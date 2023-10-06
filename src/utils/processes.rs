use anyhow::{anyhow, bail, Context, Result};
use config::LIBEXECDIR;
use glob::glob;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
    sync::OnceLock,
    time::SystemTime,
};

use gtk::{
    gio::{Icon, ThemedIcon},
    prelude::AppInfoExt,
};

use crate::{config, i18n::i18n, utils::flatpak_app_path};

use super::{is_flatpak, FLATPAK_SPAWN};

static PAGESIZE: OnceLock<usize> = OnceLock::new();

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum Containerization {
    #[default]
    None,
    Flatpak,
}

/// Represents a process that can be found within procfs.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Process {
    pub data: ProcessData,
    pub icon: Icon,
    pub cpu_time_before: u64,
    pub cpu_time_before_timestamp: u64,
    pub alive: bool,
}

/// Data that could be transferred using `resources-processes`, separated from
/// `Process` mainly due to `Icon` not being able to derive `Serialize` and
/// `Deserialize`.
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessData {
    pub pid: i32,
    pub uid: u32,
    proc_path: PathBuf,
    pub comm: String,
    pub commandline: String,
    pub cpu_time: u64,
    pub cpu_time_timestamp: u64,
    pub memory_usage: usize,
    pub cgroup: Option<String>,
    pub containerization: Containerization,
}

/// Represents an application installed on the system. It doesn't
/// have to be running (i.e. have alive processes).
#[derive(Debug, Clone)]
pub struct App {
    processes: Vec<i32>,
    pub display_name: String,
    pub description: Option<String>,
    pub icon: Icon,
    pub id: String,
    pub name: String,
}

// TODO: Better name?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    TERM,
    STOP,
    KILL,
    CONT,
}

/// Convenience struct for displaying running applications and
/// displaying a "System Processes" item.
#[derive(Debug, Clone)]
pub struct AppItem {
    pub id: Option<String>,
    pub display_name: String,
    pub icon: Icon,
    pub description: Option<String>,
    pub memory_usage: usize,
    pub cpu_time_ratio: f32,
    pub processes_amount: usize,
    pub containerization: Containerization,
}

/// Convenience struct for displaying running processes
#[derive(Debug, Clone)]
pub struct ProcessItem {
    pub pid: i32,
    pub uid: u32,
    pub display_name: String,
    pub icon: Icon,
    pub memory_usage: usize,
    pub cpu_time_ratio: f32,
    pub commandline: String,
    pub containerization: Containerization,
    pub cgroup: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AppsContext {
    apps: HashMap<String, App>,
    processes: HashMap<i32, Process>,
    processes_assigned_to_apps: HashSet<i32>,
}

impl Process {
    /// Returns a `Vec` containing all currently running processes.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems traversing and
    /// parsing procfs
    pub async fn all() -> Result<Vec<Self>> {
        let mut return_vec = Vec::new();

        if is_flatpak() {
            let proxy_path = format!(
                "{}/libexec/resources/resources-processes",
                flatpak_app_path()
            );
            let command = async_process::Command::new(FLATPAK_SPAWN)
                .args(["--host", proxy_path.as_str()])
                .output()
                .await?;
            let output = command.stdout;
            let proxy_output: Vec<ProcessData> =
                rmp_serde::from_slice::<Vec<ProcessData>>(&output)?;
            for process_data in proxy_output {
                return_vec.push(Self {
                    data: process_data,
                    icon: ThemedIcon::new("generic-process").into(),
                    cpu_time_before: 0,
                    cpu_time_before_timestamp: 0,
                    alive: true,
                });
            }
        } else {
            for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
                if let Ok(process_data) = ProcessData::try_from_path(entry).await {
                    return_vec.push(Self {
                        data: process_data,
                        icon: ThemedIcon::new("generic-process").into(),
                        cpu_time_before: 0,
                        cpu_time_before_timestamp: 0,
                        alive: true,
                    });
                }
            }
        }
        Ok(return_vec)
    }

    pub fn execute_process_action(&self, action: ProcessAction) -> Result<()> {
        let action_str = match action {
            ProcessAction::TERM => "TERM",
            ProcessAction::STOP => "STOP",
            ProcessAction::KILL => "KILL",
            ProcessAction::CONT => "CONT",
        };

        // TODO: tidy this mess up

        let kill_path = if is_flatpak() {
            format!("{}/libexec/resources/resources-kill", flatpak_app_path())
        } else {
            format!("{LIBEXECDIR}/resources-kill")
        };

        let status_code = if is_flatpak() {
            Command::new(FLATPAK_SPAWN)
                .args([
                    "--host",
                    kill_path.as_str(),
                    action_str,
                    self.data.pid.to_string().as_str(),
                ])
                .output()?
                .status
                .code()
                .with_context(|| "no status code?")?
        } else {
            Command::new(kill_path.as_str())
                .args([action_str, self.data.pid.to_string().as_str()])
                .output()?
                .status
                .code()
                .with_context(|| "no status code?")?
        };

        if status_code == 0 || status_code == 3 {
            // 0 := successful; 3 := process not found which we don't care
            // about because that might happen because we killed the
            // process' parent first, killing the child before we explicitly
            // did
            Ok(())
        } else if status_code == 1 {
            // 1 := no permissions
            self.pkexec_execute_process_action(action_str, &kill_path)
        } else {
            bail!(
                "couldn't kill {} due to unknown reasons, status code: {}",
                self.data.pid,
                status_code
            )
        }
    }

    fn pkexec_execute_process_action(&self, action: &str, kill_path: &str) -> Result<()> {
        let status_code = if is_flatpak() {
            Command::new(FLATPAK_SPAWN)
                .args([
                    "--host",
                    "pkexec",
                    "--disable-internal-agent",
                    kill_path,
                    action,
                    self.data.pid.to_string().as_str(),
                ])
                .output()?
                .status
                .code()
                .with_context(|| "no status code?")?
        } else {
            Command::new("pkexec")
                .args([
                    "--disable-internal-agent",
                    kill_path,
                    action,
                    self.data.pid.to_string().as_str(),
                ])
                .output()?
                .status
                .code()
                .with_context(|| "no status code?")?
        };

        if status_code == 0 || status_code == 3 {
            // 0 := successful; 3 := process not found which we don't care
            // about because that might happen because we killed the
            // process' parent first, killing the child before we explicitly do
            Ok(())
        } else {
            bail!(
                "couldn't kill {} with elevated privileges due to unknown reasons, status code: {}",
                self.data.pid,
                status_code
            )
        }
    }

    #[must_use]
    pub fn cpu_time_ratio(&self) -> f32 {
        if self.cpu_time_before == 0 {
            0.0
        } else {
            (self.data.cpu_time.saturating_sub(self.cpu_time_before) as f32
                / (self.data.cpu_time_timestamp - self.cpu_time_before_timestamp) as f32)
                .clamp(0.0, 1.0)
        }
    }

    pub fn sanitize_cmdline<S: AsRef<str>>(cmdline: S) -> String {
        cmdline.as_ref().replace('\0', " ")
    }

    pub async fn try_from_path(value: PathBuf) -> Result<Self> {
        Ok(Process {
            data: ProcessData::try_from_path(value.clone()).await?,
            icon: ThemedIcon::new("generic-process").into(),
            cpu_time_before: 0,
            cpu_time_before_timestamp: 0,
            alive: true,
        })
    }
}

impl ProcessData {
    fn sanitize_cgroup<S: AsRef<str>>(cgroup: S) -> Option<String> {
        let cgroups_v2_line = cgroup.as_ref().split('\n').find(|s| s.starts_with("0::"))?;
        if cgroups_v2_line.ends_with(".scope") {
            let cgroups_segments: Vec<&str> = cgroups_v2_line.split('-').collect();
            if cgroups_segments.len() > 1 {
                cgroups_segments
                    .get(cgroups_segments.len() - 2)
                    .map(|s| unescape::unescape(s).unwrap_or_else(|| (*s).to_string()))
            } else {
                None
            }
        } else if cgroups_v2_line.ends_with(".service") {
            let cgroups_segments: Vec<&str> = cgroups_v2_line.split('/').collect();
            if let Some(last) = cgroups_segments.last() {
                last[0..last.len() - 8]
                    .split('@')
                    .next()
                    .map(|s| unescape::unescape(s).unwrap_or_else(|| s.to_string()))
                    .map(|s| {
                        if s.contains("dbus-:") {
                            s.split('-').last().unwrap_or(&s).to_string()
                        } else {
                            s
                        }
                    })
            } else {
                None
            }
        } else {
            None
        }
    }

    pub async fn try_from_path(value: PathBuf) -> Result<Self> {
        let stat: Vec<String> = async_std::fs::read_to_string(value.join("stat"))
            .await?
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect();
        let statm: Vec<String> = async_std::fs::read_to_string(value.join("statm"))
            .await?
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect();
        let containerization = match &value.join("root").join(".flatpak-info").exists() {
            true => Containerization::Flatpak,
            false => Containerization::None,
        };
        Ok(Self {
            pid: value
                .file_name()
                .ok_or_else(|| anyhow!(""))?
                .to_str()
                .ok_or_else(|| anyhow!(""))?
                .parse()?,
            uid: async_std::fs::read_to_string(value.join("loginuid"))
                .await?
                .parse()?,
            comm: async_std::fs::read_to_string(value.join("comm"))
                .await
                .map(|s| s.replace('\n', ""))?,
            commandline: async_std::fs::read_to_string(value.join("cmdline"))
                .await
                .map(|s| s.replace('\0', " "))?,
            cpu_time: stat[13].parse::<u64>()? + stat[14].parse::<u64>()?,
            cpu_time_timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis() as u64,
            memory_usage: (statm[1].parse::<usize>()? - statm[2].parse::<usize>()?)
                * PAGESIZE.get_or_init(sysconf::pagesize),
            cgroup: Self::sanitize_cgroup(
                async_std::fs::read_to_string(value.join("cgroup")).await?,
            ),
            proc_path: value,
            containerization,
        })
    }
}

impl App {
    pub fn refresh(&mut self, apps: &mut AppsContext) {
        self.processes = self
            .processes_iter_mut(apps)
            .filter_map(|p| if p.alive { Some(p.data.pid) } else { None })
            .collect();
    }

    /// Adds a process to the processes `HashMap` and also
    /// updates the `Process`' icon to the one of this
    /// `App`
    pub fn add_process(&mut self, process: &mut Process) {
        process.icon = self.icon.clone();
        self.processes.push(process.data.pid);
    }

    pub fn remove_process(&mut self, process: &Process) {
        self.processes.retain(|p| *p != process.data.pid);
    }

    #[must_use]
    pub fn is_running(&self, apps: &AppsContext) -> bool {
        self.processes_iter(apps).count() > 0
    }

    pub fn processes_iter<'a>(&'a self, apps: &'a AppsContext) -> impl Iterator<Item = &Process> {
        apps.all_processes()
            .filter(move |process| self.processes.contains(&process.data.pid) && process.alive)
    }

    pub fn processes_iter_mut<'a>(
        &'a mut self,
        apps: &'a mut AppsContext,
    ) -> impl Iterator<Item = &mut Process> {
        apps.all_processes_mut()
            .filter(move |process| self.processes.contains(&process.data.pid) && process.alive)
    }

    #[must_use]
    pub fn memory_usage(&self, apps: &AppsContext) -> usize {
        self.processes_iter(apps)
            .map(|process| process.data.memory_usage)
            .sum()
    }

    #[must_use]
    pub fn cpu_time(&self, apps: &AppsContext) -> u64 {
        self.processes_iter(apps)
            .map(|process| process.data.cpu_time)
            .sum()
    }

    #[must_use]
    pub fn cpu_time_timestamp(&self, apps: &AppsContext) -> u64 {
        self.processes_iter(apps)
            .map(|process| process.data.cpu_time_timestamp)
            .sum::<u64>()
            .checked_div(self.processes.len() as u64) // the timestamps of the last cpu time check should be pretty much equal but to be sure, take the average of all of them
            .unwrap_or(0)
    }

    #[must_use]
    pub fn cpu_time_before(&self, apps: &AppsContext) -> u64 {
        self.processes_iter(apps)
            .map(|process| process.cpu_time_before)
            .sum()
    }

    #[must_use]
    pub fn cpu_time_before_timestamp(&self, apps: &AppsContext) -> u64 {
        self.processes_iter(apps)
            .map(|process| process.cpu_time_before_timestamp)
            .sum::<u64>()
            .checked_div(self.processes.len() as u64)
            .unwrap_or(0)
    }

    #[must_use]
    pub fn cpu_time_ratio(&self, apps: &AppsContext) -> f32 {
        self.processes_iter(apps)
            .map(Process::cpu_time_ratio)
            .sum::<f32>()
            .clamp(0.0, 1.0)
    }

    pub fn execute_process_action(
        &self,
        apps: &AppsContext,
        action: ProcessAction,
    ) -> Vec<Result<()>> {
        self.processes_iter(apps)
            .map(|process| process.execute_process_action(action))
            .collect()
    }
}

impl AppsContext {
    /// Creates a new `Apps` object, this operation is quite expensive
    /// so try to do it only one time during the lifetime of the program.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems getting the list of
    /// running processes.
    pub async fn new() -> Result<AppsContext> {
        let mut apps = HashMap::new();
        let app_infos = gtk::gio::AppInfo::all();
        // turn AppInfo into App
        for app_info in app_infos {
            if let Some(id) = app_info
                .id()
                .map(|gstring| Self::sanitize_appid(gstring.to_string()))
            {
                apps.insert(
                    id.clone(),
                    App {
                        processes: Vec::new(),
                        display_name: app_info.display_name().to_string(),
                        description: app_info.description().map(|gs| gs.to_string()),
                        id,
                        name: app_info.name().to_string(),
                        icon: app_info
                            .icon()
                            .unwrap_or_else(|| ThemedIcon::new("generic-process").into()),
                    },
                );
            }
        }

        let mut processes = HashMap::new();
        let processes_list = Process::all().await?;
        let mut processes_assigned_to_apps = HashSet::new();

        for mut process in processes_list {
            if let Some(app) = apps.get_mut(process.data.cgroup.as_deref().unwrap_or_default()) {
                processes_assigned_to_apps.insert(process.data.pid);
                app.add_process(&mut process);
            }
            processes.insert(process.data.pid, process);
        }

        Ok(AppsContext {
            apps,
            processes,
            processes_assigned_to_apps,
        })
    }

    pub fn get_process(&self, pid: i32) -> Option<&Process> {
        self.processes.get(&pid)
    }

    fn sanitize_appid<S: Into<String>>(a: S) -> String {
        let mut appid: String = a.into();
        if appid.ends_with(".desktop") {
            appid = appid[0..appid.len() - 8].to_string();
        }
        appid
    }

    pub fn get_app(&self, id: &str) -> Option<&App> {
        self.apps.get(id)
    }

    #[must_use]
    pub fn all_processes(&self) -> impl Iterator<Item = &Process> {
        self.processes.values().filter(|p| p.alive)
    }

    #[must_use]
    pub fn all_processes_mut(&mut self) -> impl Iterator<Item = &mut Process> {
        self.processes.values_mut().filter(|p| p.alive)
    }

    /// Returns a `HashMap` of running processes. For more info, refer to
    /// `ProcessItem`.
    pub fn process_items(&self) -> HashMap<i32, ProcessItem> {
        self.all_processes()
            .filter(|process| !process.data.commandline.is_empty()) // find a way to display procs without commandlines
            .map(|process| {
                (
                    process.data.pid,
                    self.process_item(process.data.pid).unwrap(),
                )
            })
            .collect()
    }

    pub fn process_item(&self, pid: i32) -> Option<ProcessItem> {
        self.get_process(pid).map(|process| ProcessItem {
            pid: process.data.pid,
            display_name: process.data.comm.clone(),
            icon: process.icon.clone(),
            memory_usage: process.data.memory_usage,
            cpu_time_ratio: process.cpu_time_ratio(),
            commandline: Process::sanitize_cmdline(process.data.commandline.clone()),
            containerization: process.data.containerization.clone(),
            cgroup: process.data.cgroup.clone(),
            uid: process.data.uid,
        })
    }

    /// Returns a `HashMap` of running graphical applications. For more info,
    /// refer to `AppItem`.
    #[must_use]
    pub fn app_items(&self) -> HashMap<Option<String>, AppItem> {
        let mut app_pids = HashSet::new();

        let mut return_map = self
            .apps
            .iter()
            .filter(|(_, app)| app.is_running(self) && !app.id.starts_with("xdg-desktop-portal"))
            .map(|(_, app)| {
                app.processes_iter(self).for_each(|process| {
                    app_pids.insert(process.data.pid);
                });

                let containerization = if app
                    .processes_iter(self)
                    .filter(|process| {
                        !process.data.commandline.starts_with("bwrap")
                            && !process.data.commandline.is_empty()
                    })
                    .any(|process| process.data.containerization == Containerization::Flatpak)
                {
                    Containerization::Flatpak
                } else {
                    Containerization::None
                };

                (
                    Some(app.id.clone()),
                    AppItem {
                        id: Some(app.id.clone()),
                        display_name: app.display_name.clone(),
                        icon: app.icon.clone(),
                        description: app.description.clone(),
                        memory_usage: app.memory_usage(self),
                        cpu_time_ratio: app.cpu_time_ratio(self),
                        processes_amount: app.processes_iter(self).count(),
                        containerization,
                    },
                )
            })
            .collect::<HashMap<Option<String>, AppItem>>();

        let system_cpu_ratio = self
            .all_processes()
            .filter(|process| !app_pids.contains(&process.data.pid) && process.alive)
            .map(Process::cpu_time_ratio)
            .sum();

        let system_memory_usage: usize = self
            .all_processes()
            .filter(|process| !app_pids.contains(&process.data.pid) && process.alive)
            .map(|process| process.data.memory_usage)
            .sum();

        return_map.insert(
            None,
            AppItem {
                id: None,
                display_name: i18n("System Processes"),
                icon: ThemedIcon::new("system-processes").into(),
                description: None,
                memory_usage: system_memory_usage,
                cpu_time_ratio: system_cpu_ratio,
                processes_amount: self.processes.len(),
                containerization: Containerization::None,
            },
        );
        return_map
    }

    /// Refreshes the statistics about the running applications and processes.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems getting the new list of
    /// running processes or if there are anomalies in a process procfs
    /// directory.
    pub async fn refresh(&mut self) -> Result<()> {
        let newly_gathered_processes = Process::all().await?;
        let mut updated_processes = HashSet::new();

        for mut refreshed_process in newly_gathered_processes {
            updated_processes.insert(refreshed_process.data.pid);
            // refresh our old processes
            if let Some(old_process) = self.processes.get_mut(&refreshed_process.data.pid) {
                old_process.cpu_time_before = old_process.data.cpu_time;
                old_process.cpu_time_before_timestamp = old_process.data.cpu_time_timestamp;
                old_process.data = refreshed_process.data.clone();
            } else {
                // this is a new process, see if it belongs to a graphical app

                if self
                    .processes_assigned_to_apps
                    .contains(&refreshed_process.data.pid)
                {
                    continue;
                }

                if let Some(app) = self
                    .apps
                    .get_mut(refreshed_process.data.cgroup.as_deref().unwrap_or_default())
                {
                    self.processes_assigned_to_apps
                        .insert(refreshed_process.data.pid);
                    app.add_process(&mut refreshed_process);
                }

                self.processes
                    .insert(refreshed_process.data.pid, refreshed_process);
            }
        }

        // all the not-updated processes have unfortunately died, probably
        for process in self.processes.values_mut() {
            if !updated_processes.contains(&process.data.pid) {
                process.alive = false;
            }
        }

        Ok(())
    }
}
