use anyhow::{anyhow, bail, Context, Result};
use config::LIBEXECDIR;
use glob::glob;
use log::debug;
use nix::{sys::signal, unistd::Pid};
use once_cell::sync::OnceCell;
use std::{collections::HashMap, path::PathBuf, process::Command, time::SystemTime};

use gtk::{
    gio::{AppInfo, Icon, ThemedIcon},
    prelude::AppInfoExt,
};

use crate::{config, i18n::i18n};

static PAGESIZE: OnceCell<usize> = OnceCell::new();

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub enum Containerization {
    #[default]
    None,
    Flatpak,
}

/// Represents a process that can be found within procfs.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Process {
    pub pid: i32,
    pub uid: u32,
    proc_path: PathBuf,
    pub comm: String,
    pub commandline: String,
    pub cpu_time: u64,
    pub cpu_time_timestamp: u64,
    pub cpu_time_before: u64,
    pub cpu_time_before_timestamp: u64,
    pub memory_usage: usize,
    pub cgroup: Option<String>,
    alive: bool,
    pub containerization: Containerization,
    pub icon: Icon,
}

/// Represents an application installed on the system. It doesn't
/// have to be running (i.e. have alive processes).
#[derive(Debug, Clone)]
pub struct App {
    processes: HashMap<i32, Process>,
    app_info: AppInfo,
}

/// Convenience struct for displaying running applications and
/// displaying a "System Processes" item.
#[derive(Debug, Clone)]
pub struct SimpleItem {
    pub id: Option<String>,
    pub display_name: String,
    pub icon: Icon,
    pub description: Option<String>,
    pub executable: Option<PathBuf>,
    pub memory_usage: usize,
    pub cpu_time_ratio: f32,
    pub processes_amount: usize,
    pub containerization: Containerization,
}

#[derive(Debug, Clone, Default)]
pub struct Apps {
    apps: HashMap<String, App>,
    system_processes: Vec<Process>,
    known_proc_paths: Vec<PathBuf>,
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
        for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
            return_vec.push(Self::try_from_path(entry).await);
        }
        Ok(return_vec.into_iter().flatten().collect())
    }

    async fn refresh_result(&mut self) -> Result<()> {
        let stat: Vec<String> = async_std::fs::read_to_string(self.proc_path.join("stat"))
            .await?
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect();
        let statm: Vec<String> = async_std::fs::read_to_string(self.proc_path.join("statm"))
            .await?
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect();
        self.cpu_time_before = self.cpu_time;
        self.cpu_time_before_timestamp = self.cpu_time_timestamp;
        self.cpu_time = stat[13].parse::<u64>()? + stat[14].parse::<u64>()?;
        self.cpu_time_timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis() as u64;
        self.memory_usage = (statm[1].parse::<usize>()? - statm[2].parse::<usize>()?)
            * PAGESIZE.get_or_init(sysconf::pagesize);
        Ok(())
    }

    pub async fn refresh(&mut self) -> bool {
        self.alive = self.proc_path.exists() && self.refresh_result().await.is_ok();
        self.alive
    }

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

    /// Terminates the processes
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems terminating the process
    /// that running it with superuser permissions can't solve
    pub fn term(&self) -> Result<()> {
        debug!("sending SIGTERM to pid {}", self.pid);
        let result = signal::kill(Pid::from_raw(self.pid), Some(signal::Signal::SIGTERM));
        if let Err(err) = result {
            return match err {
                nix::errno::Errno::EPERM => self.pkexec_term(),
                _ => bail!("unable to term {}", self.pid),
            };
        }
        Ok(())
    }

    fn pkexec_term(&self) -> Result<()> {
        debug!(
            "using pkexec to send SIGTERM with root privileges to pid {}",
            self.pid
        );
        let path = format!("{LIBEXECDIR}/resources-kill");
        Command::new("pkexec")
            .args([
                "--disable-internal-agent",
                &path,
                "TERM",
                self.pid.to_string().as_str(),
            ])
            .spawn()
            .map(|_| ())
            .with_context(|| format!("failure calling {} on {} (with pkexec)", &path, self.pid))
    }

    /// Kills the process
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems killing the process
    /// that running it with superuser permissions can't solve
    pub fn kill(&self) -> Result<()> {
        debug!("sending SIGKILL to pid {}", self.pid);
        let result = signal::kill(Pid::from_raw(self.pid), Some(signal::Signal::SIGKILL));
        if let Err(err) = result {
            return match err {
                nix::errno::Errno::EPERM => self.pkexec_kill(),
                _ => bail!("unable to kill {}", self.pid),
            };
        }
        Ok(())
    }

    fn pkexec_kill(&self) -> Result<()> {
        debug!(
            "using pkexec to send SIGKILL with root privileges to pid {}",
            self.pid
        );
        let path = format!("{LIBEXECDIR}/resources-kill");
        Command::new("pkexec")
            .args([
                "--disable-internal-agent",
                &path,
                "KILL",
                self.pid.to_string().as_str(),
            ])
            .spawn()
            .map(|_| ())
            .with_context(|| format!("failure calling {} on {} (with pkexec)", &path, self.pid))
    }

    /// Stops the process
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems stopping the process
    /// that running it with superuser permissions can't solve
    pub fn stop(&self) -> Result<()> {
        debug!("sending SIGSTOP to pid {}", self.pid);
        let result = signal::kill(Pid::from_raw(self.pid), Some(signal::Signal::SIGSTOP));
        if let Err(err) = result {
            return match err {
                nix::errno::Errno::EPERM => self.pkexec_stop(),
                _ => bail!("unable to stop {}", self.pid),
            };
        }
        Ok(())
    }

    fn pkexec_stop(&self) -> Result<()> {
        debug!(
            "using pkexec to send SIGSTOP with root privileges to pid {}",
            self.pid
        );
        let path = format!("{LIBEXECDIR}/resources-kill");
        Command::new("pkexec")
            .args([
                "--disable-internal-agent",
                &path,
                "STOP",
                self.pid.to_string().as_str(),
            ])
            .spawn()
            .map(|_| ())
            .with_context(|| format!("failure calling {} on {} (with pkexec)", &path, self.pid))
    }

    /// Continues the processes
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems continuing the process
    /// that running it with superuser permissions can't solve
    pub fn cont(&self) -> Result<()> {
        debug!("sending SIGCONT to pid {}", self.pid);
        let result = signal::kill(Pid::from_raw(self.pid), Some(signal::Signal::SIGCONT));
        if let Err(err) = result {
            return match err {
                nix::errno::Errno::EPERM => self.pkexec_cont(),
                _ => bail!("unable to cont {}", self.pid),
            };
        }
        Ok(())
    }

    fn pkexec_cont(&self) -> Result<()> {
        debug!(
            "using pkexec to send SIGCONT with root privileges to pid {}",
            self.pid
        );
        let path = format!("{LIBEXECDIR}/resources-kill");
        Command::new("pkexec")
            .args([
                "--disable-internal-agent",
                &path,
                "CONT",
                self.pid.to_string().as_str(),
            ])
            .spawn()
            .map(|_| ())
            .with_context(|| format!("failure calling {} on {} (with pkexec)", &path, self.pid))
    }

    #[must_use]
    pub fn cpu_time_ratio(&self) -> f32 {
        if self.cpu_time_before == 0 {
            0.0
        } else {
            (self.cpu_time.saturating_sub(self.cpu_time_before) as f32
                / (self.cpu_time_timestamp - self.cpu_time_before_timestamp) as f32)
                .clamp(0.0, 1.0)
        }
    }

    pub fn sanitize_cmdline<S: AsRef<str>>(cmdline: S) -> String {
        cmdline.as_ref().replace('\0', " ")
    }

    async fn try_from_path(value: PathBuf) -> Result<Self> {
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
        Ok(Process {
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
            cpu_time_before: 0,
            cpu_time_before_timestamp: 0,
            memory_usage: (statm[1].parse::<usize>()? - statm[2].parse::<usize>()?)
                * PAGESIZE.get_or_init(sysconf::pagesize),
            cgroup: Self::sanitize_cgroup(
                async_std::fs::read_to_string(value.join("cgroup")).await?,
            ),
            proc_path: value,
            alive: true,
            containerization,
            icon: ThemedIcon::new("generic-process").into(),
        })
    }
}

impl App {
    /// Adds a process to the processes `HashMap` and also
    /// updates the `Process`' icon to the one of this
    /// `App`
    pub fn add_process(&mut self, mut process: Process) {
        process.icon = self.icon();
        self.processes.insert(process.pid, process);
    }

    #[must_use]
    pub fn commandline(&self) -> Option<PathBuf> {
        self.app_info.commandline()
    }

    #[must_use]
    pub fn description(&self) -> Option<String> {
        self.app_info.description().map(|x| x.to_string())
    }

    #[must_use]
    pub fn display_name(&self) -> String {
        self.app_info.display_name().to_string()
    }

    #[must_use]
    pub fn executable(&self) -> PathBuf {
        self.app_info.executable()
    }

    #[must_use]
    pub fn icon(&self) -> Icon {
        if let Some(id) = self.id() && id == "org.gnome.Shell" {
            return ThemedIcon::new("shell").into();
        }
        self.app_info
            .icon()
            .unwrap_or_else(|| ThemedIcon::new("generic-process").into())
    }

    #[must_use]
    pub fn id(&self) -> Option<String> {
        self.app_info
            .id()
            .map(|id| Apps::sanitize_appid(id.to_string()))
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.app_info.name().to_string()
    }

    pub async fn refresh(&mut self) -> Vec<PathBuf> {
        let mut dead_processes = Vec::new();
        for process in self.processes.values_mut() {
            if !process.refresh().await {
                dead_processes.push(process.proc_path.clone());
            }
        }
        self.processes
            .retain(|_, process| !dead_processes.contains(&process.proc_path));
        dead_processes
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        !self.processes.is_empty()
    }

    #[must_use]
    pub fn memory_usage(&self) -> usize {
        self.processes
            .values()
            .map(|process| process.memory_usage)
            .sum()
    }

    #[must_use]
    pub fn cpu_time(&self) -> u64 {
        self.processes
            .values()
            .map(|process| process.cpu_time)
            .sum()
    }

    #[must_use]
    pub fn cpu_time_timestamp(&self) -> u64 {
        self.processes
            .values()
            .map(|process| process.cpu_time_timestamp)
            .sum::<u64>()
            .checked_div(self.processes.len() as u64) // the timestamps of the last cpu time check should be pretty much equal but to be sure, take the average of all of them
            .unwrap_or(0)
    }

    #[must_use]
    pub fn cpu_time_before(&self) -> u64 {
        self.processes
            .values()
            .map(|process| process.cpu_time_before)
            .sum()
    }

    #[must_use]
    pub fn cpu_time_before_timestamp(&self) -> u64 {
        self.processes
            .values()
            .map(|process| process.cpu_time_before_timestamp)
            .sum::<u64>()
            .checked_div(self.processes.len() as u64)
            .unwrap_or(0)
    }

    #[must_use]
    pub fn cpu_time_ratio(&self) -> f32 {
        if self.cpu_time_before() == 0 {
            0.0
        } else {
            ((self.cpu_time().saturating_sub(self.cpu_time_before())) as f32
                / (self.cpu_time_timestamp() - self.cpu_time_before_timestamp()) as f32)
                .clamp(0.0, 1.0)
        }
    }

    #[must_use]
    pub fn term(&self) -> Vec<Result<()>> {
        debug!("sending SIGTERM to processes of {}", self.display_name());
        self.processes.values().map(Process::term).collect()
    }

    #[must_use]
    pub fn kill(&self) -> Vec<Result<()>> {
        debug!("sending SIGKILL to processes of {}", self.display_name());
        self.processes.values().map(Process::kill).collect()
    }

    #[must_use]
    pub fn stop(&self) -> Vec<Result<()>> {
        debug!("sending SIGSTOP to processes of {}", self.display_name());
        self.processes.values().map(Process::stop).collect()
    }

    #[must_use]
    pub fn cont(&self) -> Vec<Result<()>> {
        debug!("sending SIGCONT to processes of {}", self.display_name());
        self.processes.values().map(Process::cont).collect()
    }
}

impl Apps {
    /// Creates a new `Apps` object, this operation is quite expensive
    /// so try to do it only one time during the lifetime of the program.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems getting the list of
    /// running processes.
    pub async fn new() -> Result<Self> {
        let app_infos = gtk::gio::AppInfo::all();
        let mut app_map = HashMap::new();
        let mut processes = Process::all().await?;
        let mut known_proc_paths = Vec::new();
        for app_info in app_infos {
            if let Some(id) = app_info
                .id()
                .map(|gstring| Self::sanitize_appid(gstring.to_string()))
            {
                app_map.insert(
                    id.clone(),
                    App {
                        processes: HashMap::new(),
                        app_info,
                    },
                );
            }
        }
        processes
            .iter()
            .for_each(|process| known_proc_paths.push(process.proc_path.clone()));
        // split the processes `Vec` into two `Vec`s:
        // one, where the process' cgroup can be found as an ID
        // of the installed graphical applications (meaning that the
        // process belongs to a graphical application) and one where
        // this is not possible. the latter are our system processes
        let non_system_processes: Vec<Process> = processes
            .drain_filter(|process| {
                process
                    .cgroup
                    .as_deref()
                    .map_or(true, |cgroup| !cgroup.starts_with("xdg-desktop-portal")) // throw out the portals
                    && app_map.contains_key(process.cgroup.as_deref().unwrap_or_default())
            })
            .collect();
        for process in non_system_processes {
            if let Some(app) = app_map.get_mut(process.cgroup.as_deref().unwrap_or_default()) {
                app.add_process(process);
            }
        }
        Ok(Apps {
            apps: app_map,
            system_processes: processes,
            known_proc_paths,
        })
    }

    fn sanitize_appid<S: Into<String>>(a: S) -> String {
        let mut appid: String = a.into();
        if appid.ends_with(".desktop") {
            appid = appid[0..appid.len() - 8].to_string();
        }
        appid
    }

    pub fn get_app<S: AsRef<str>>(&self, id: S) -> Option<&App> {
        self.apps.get(id.as_ref())
    }

    #[must_use]
    pub fn system_processes(&self) -> &Vec<Process> {
        &self.system_processes
    }

    #[must_use]
    pub fn all_processes(&self) -> Vec<Process> {
        self.system_processes
            .iter()
            .chain(self.apps.values().flat_map(|app| app.processes.values()))
            .cloned()
            .collect()
    }

    /// Returns a `Vec` of running graphical applications. For more
    /// info, refer to `SimpleItem`.
    #[must_use]
    pub fn simple(&self) -> Vec<SimpleItem> {
        let mut return_vec = self
            .apps
            .iter()
            .filter(|(_, app)| app.is_running())
            .map(|(_, app)| {
                let containerization = if app
                    .processes
                    .values()
                    .filter(|process| {
                        !process.commandline.starts_with("bwrap") && !process.commandline.is_empty()
                    })
                    .all(|process| process.containerization == Containerization::Flatpak)
                {
                    Containerization::Flatpak
                } else {
                    Containerization::None
                };

                SimpleItem {
                    id: app.id(),
                    display_name: app.display_name(),
                    icon: app.icon(),
                    description: app.description(),
                    executable: Some(app.executable()),
                    memory_usage: app.memory_usage(),
                    cpu_time_ratio: app.cpu_time_ratio(),
                    processes_amount: app.processes.len(),
                    containerization,
                }
            })
            .collect::<Vec<SimpleItem>>();
        let system_cpu_time: u64 = self
            .system_processes
            .iter()
            .map(|process| process.cpu_time)
            .sum();
        let system_cpu_time_timestamp = self
            .system_processes
            .iter()
            .map(|process| process.cpu_time_timestamp)
            .sum::<u64>()
            .checked_div(self.system_processes.len() as u64)
            .unwrap_or(0);
        let system_cpu_time_before: u64 = self
            .system_processes
            .iter()
            .map(|process| process.cpu_time_before)
            .sum();
        let system_cpu_time_before_timestamp = self
            .system_processes
            .iter()
            .map(|process| process.cpu_time_before_timestamp)
            .sum::<u64>()
            .checked_div(self.system_processes.len() as u64)
            .unwrap_or(0);
        let system_cpu_ratio = if system_cpu_time_before == 0 {
            0.0
        } else {
            (system_cpu_time.saturating_sub(system_cpu_time_before) as f32
                / (system_cpu_time_timestamp - system_cpu_time_before_timestamp) as f32)
                .clamp(0.0, 1.0)
        };
        return_vec.push(SimpleItem {
            id: None,
            display_name: i18n("System Processes"),
            icon: ThemedIcon::new("system-processes").into(),
            description: None,
            executable: None,
            memory_usage: self
                .system_processes
                .iter()
                .map(|process| process.memory_usage)
                .sum(),
            cpu_time_ratio: system_cpu_ratio,
            processes_amount: self.system_processes.len(),
            containerization: Containerization::None,
        });
        return_vec
    }

    /// Refreshes the statistics about the running applications and processes.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems getting the new list of
    /// running processes or if there are anomalies in a process procfs
    /// directory.
    pub async fn refresh(&mut self) -> Result<()> {
        // look for processes that might have died since we last checked
        // and update the stats of the processes that are still alive
        // while we're at it
        let mut dead_app_processes = Vec::new();
        for app in self.apps.values_mut() {
            dead_app_processes.extend(app.refresh().await);
        }
        let mut dead_sys_processes = Vec::new();
        for process in self.system_processes.iter_mut() {
            if !process.refresh().await {
                dead_sys_processes.push(process.proc_path.clone())
            }
        }
        self.system_processes
            .retain(|process| !dead_sys_processes.contains(&process.proc_path));

        // now get the processes that might have been added:
        for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
            // is the current proc_path already known?
            if self
                .known_proc_paths
                .iter()
                .any(|proc_path| *proc_path == entry)
            {
                // if so, we can continue
                continue;
            }
            // if not, insert it into our known_proc_paths
            let process = Process::try_from_path(entry.clone()).await?;
            if let Some(app) = self
                .apps
                .get_mut(process.cgroup.as_deref().unwrap_or_default())
            {
                app.processes.insert(process.pid, process);
            } else {
                self.system_processes.push(process);
            }
            self.known_proc_paths.push(entry);
        }

        // we still have to remove the processes that died from
        // known_proc_paths
        for dead_process in dead_app_processes.iter().chain(dead_sys_processes.iter()) {
            if let Some(pos) = self
                .known_proc_paths
                .iter()
                .position(|x| *x == *dead_process)
            {
                self.known_proc_paths.swap_remove(pos);
            }
        }
        Ok(())
    }
}
