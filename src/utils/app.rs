use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gtk::gio::{Icon, ThemedIcon};
use hashbrown::{HashMap, HashSet};
use once_cell::sync::Lazy;
use process_data::{Containerization, ProcessData};

use crate::i18n::i18n;

use super::process::{Process, ProcessAction, ProcessItem};

// Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
static DATA_DIRS: Lazy<Vec<PathBuf>> = Lazy::new(|| {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let mut data_dirs: Vec<PathBuf> = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| format!("/usr/share:{}/.local/share", home))
        .split(':')
        .map(PathBuf::from)
        .collect();
    data_dirs.push(PathBuf::from(format!("{}/.local/share", home)));
    data_dirs
});

// This contains known occurences of processes having a too distinct name from the actual app
// The HashMap is used like this:
//   Key: The name of the executable of the process
//   Value: What it should be replaced with when finding out to which app it belongs
static KNOWN_EXECUTABLE_NAME_EXCEPTIONS: Lazy<HashMap<String, String>> = Lazy::new(|| {
    HashMap::from([
        ("firefox-bin".into(), "firefox".into()),
        ("oosplash".into(), "libreoffice".into()),
        ("soffice.bin".into(), "libreoffice".into()),
    ])
});

#[derive(Debug, Clone, Default)]
pub struct AppsContext {
    apps: HashMap<String, App>,
    processes: HashMap<i32, Process>,
    processes_assigned_to_apps: HashSet<i32>,
    read_bytes_from_dead_processes: u64,
    write_bytes_from_dead_processes: u64,
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
    pub read_speed: f64,
    pub read_total: u64,
    pub write_speed: f64,
    pub write_total: u64,
}

/// Represents an application installed on the system. It doesn't
/// have to be running (i.e. have alive processes).
#[derive(Debug, Clone)]
pub struct App {
    processes: Vec<i32>,
    pub commandline: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub icon: Icon,
    pub id: String,
    pub read_bytes_from_dead_processes: u64,
    pub write_bytes_from_dead_processes: u64,
}

impl App {
    pub fn all() -> Vec<App> {
        DATA_DIRS
            .iter()
            .flat_map(|path| {
                let applications_path = path.join("applications");
                let expanded_path = expanduser::expanduser(applications_path.to_string_lossy())
                    .unwrap_or(applications_path);
                expanded_path.read_dir().ok().map(|read| {
                    read.filter_map(|file_res| {
                        file_res
                            .ok()
                            .and_then(|file| Self::from_desktop_file(file.path()).ok())
                    })
                })
            })
            .flatten()
            .collect()
    }

    pub fn from_desktop_file<P: AsRef<Path>>(file_path: P) -> Result<App> {
        let ini = ini::Ini::load_from_file(file_path.as_ref())?;

        let desktop_entry = ini
            .section(Some("Desktop Entry"))
            .context("no desktop entry section")?;

        let id = desktop_entry
            .get("X-Flatpak") // is there a X-Flatpak section?
            .map(str::to_string)
            .or_else(|| {
                // if not, presume that the ID is in the file name
                Some(
                    file_path
                        .as_ref()
                        .file_stem()?
                        .to_string_lossy()
                        .to_string(),
                )
            })
            .context("unable to get ID of desktop file")?;

        Ok(App {
            commandline: desktop_entry.get("Exec").map(str::to_string),
            processes: Vec::new(),
            display_name: desktop_entry.get("Name").unwrap_or(&id).to_string(),
            description: desktop_entry.get("Comment").map(str::to_string),
            icon: ThemedIcon::new(desktop_entry.get("Icon").unwrap_or("generic-process")).into(),
            id,
            read_bytes_from_dead_processes: 0,
            write_bytes_from_dead_processes: 0,
        })
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
    pub fn is_running(&self) -> bool {
        !self.processes.is_empty()
    }

    pub fn processes_iter<'a>(&'a self, apps: &'a AppsContext) -> impl Iterator<Item = &Process> {
        apps.all_processes()
            .filter(move |process| self.processes.contains(&process.data.pid))
    }

    pub fn processes_iter_mut<'a>(
        &'a mut self,
        apps: &'a mut AppsContext,
    ) -> impl Iterator<Item = &mut Process> {
        apps.all_processes_mut()
            .filter(move |process| self.processes.contains(&process.data.pid))
    }

    #[must_use]
    pub fn memory_usage(&self, apps: &AppsContext) -> usize {
        self.processes_iter(apps)
            .map(|process| process.data.memory_usage)
            .sum()
    }

    #[must_use]
    pub fn cpu_time_ratio(&self, apps: &AppsContext) -> f32 {
        self.processes_iter(apps)
            .map(Process::cpu_time_ratio)
            .sum::<f32>()
            .clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn read_speed(&self, apps: &AppsContext) -> f64 {
        self.processes_iter(apps)
            .filter_map(Process::read_speed)
            .sum()
    }

    #[must_use]
    pub fn read_total(&self, apps: &AppsContext) -> u64 {
        self.read_bytes_from_dead_processes.saturating_add(
            self.processes_iter(apps)
                .filter_map(|process| process.data.read_bytes)
                .sum::<u64>(),
        )
    }

    #[must_use]
    pub fn write_speed(&self, apps: &AppsContext) -> f64 {
        self.processes_iter(apps)
            .filter_map(Process::write_speed)
            .sum()
    }

    #[must_use]
    pub fn write_total(&self, apps: &AppsContext) -> u64 {
        self.write_bytes_from_dead_processes.saturating_add(
            self.processes_iter(apps)
                .filter_map(|process| process.data.write_bytes)
                .sum::<u64>(),
        )
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
    /// Creates a new `AppsContext` object, this operation is quite expensive
    /// so try to do it only one time during the lifetime of the program.
    /// Please call refresh() immediately after this function.
    pub async fn new() -> AppsContext {
        let apps: HashMap<String, App> = App::all()
            .into_iter()
            .map(|app| (app.id.clone(), app))
            .collect();

        AppsContext {
            apps,
            processes: HashMap::new(),
            processes_assigned_to_apps: HashSet::new(),
            read_bytes_from_dead_processes: 0,
            write_bytes_from_dead_processes: 0,
        }
    }

    fn app_associated_with_process(&mut self, process: &Process) -> Option<String> {
        // TODO: tidy this up
        // ↓ look for whether we can find an ID in the cgroup
        if let Some(app) = self
            .apps
            .get(process.data.cgroup.as_deref().unwrap_or_default())
        {
            Some(app.id.clone())
        } else if let Some(app) = self.apps.get(&process.executable_path) {
            // ↑ look for whether we can find an ID in the executable path of the process
            Some(app.id.clone())
        } else if let Some(app) = self.apps.get(&process.executable_name) {
            // ↑ look for whether we can find an ID in the executable name of the process
            Some(app.id.clone())
        } else {
            self.apps
                .values()
                .find(|a| {
                    // ↓ probably most expensive lookup, therefore only last resort: look for whether the process' commandline
                    //   can be found in the apps' commandline
                    a.commandline
                        .as_ref()
                        .map(|app_commandline| {
                            let app_executable_name = app_commandline
                                .split(' ') // filter any arguments (e. g. from "/usr/bin/firefox %u" to "/usr/bin/firefox")
                                .nth(0)
                                .unwrap_or_default()
                                .split('/') // filter the executable path (e. g. from "/usr/bin/firefox" to "firefox")
                                .nth_back(0)
                                .unwrap_or_default();
                            app_commandline == &process.executable_path
                                || app_executable_name == process.executable_name
                                || KNOWN_EXECUTABLE_NAME_EXCEPTIONS
                                    .get(&process.executable_name)
                                    .map(|sub_executable_name| {
                                        sub_executable_name == app_executable_name
                                    })
                                    .unwrap_or(false)
                        })
                        .unwrap_or(false)
                })
                .map(|app| app.id.clone())
        }
    }

    pub fn get_process(&self, pid: i32) -> Option<&Process> {
        self.processes.get(&pid)
    }

    pub fn get_app(&self, id: &str) -> Option<&App> {
        self.apps.get(id)
    }

    #[must_use]
    pub fn all_processes(&self) -> impl Iterator<Item = &Process> {
        self.processes.values()
    }

    #[must_use]
    pub fn all_processes_mut(&mut self) -> impl Iterator<Item = &mut Process> {
        self.processes.values_mut()
    }

    /// Returns a `HashMap` of running processes. For more info, refer to
    /// `ProcessItem`.
    pub fn process_items(&self) -> HashMap<i32, ProcessItem> {
        self.all_processes()
            .map(|process| (process.data.pid, self.process_item(process.data.pid)))
            .filter_map(|(pid, process_opt)| process_opt.map(|process| (pid, process)))
            .collect()
    }

    pub fn process_item(&self, pid: i32) -> Option<ProcessItem> {
        self.get_process(pid).map(|process| {
            let full_comm = if process.executable_name.starts_with(&process.data.comm) {
                process.executable_name.clone()
            } else {
                process.data.comm.clone()
            };
            ProcessItem {
                pid: process.data.pid,
                display_name: full_comm.clone(),
                icon: process.icon.clone(),
                memory_usage: process.data.memory_usage,
                cpu_time_ratio: process.cpu_time_ratio(),
                commandline: Process::sanitize_cmdline(process.data.commandline.clone())
                    .unwrap_or(full_comm),
                containerization: process.data.containerization.clone(),
                cgroup: process.data.cgroup.clone(),
                uid: process.data.uid,
                read_speed: process.read_speed(),
                read_total: process.data.read_bytes,
                write_speed: process.write_speed(),
                write_total: process.data.write_bytes,
            }
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
            .filter(|(_, app)| app.is_running() && !app.id.starts_with("xdg-desktop-portal"))
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
                        read_speed: app.read_speed(self),
                        read_total: app.read_total(self),
                        write_speed: app.write_speed(self),
                        write_total: app.write_total(self),
                    },
                )
            })
            .collect::<HashMap<Option<String>, AppItem>>();

        let system_cpu_ratio = self
            .system_processes_iter()
            .map(Process::cpu_time_ratio)
            .sum();

        let system_memory_usage: usize = self
            .system_processes_iter()
            .map(|process| process.data.memory_usage)
            .sum();

        let system_read_speed = self
            .system_processes_iter()
            .filter_map(|process| process.read_speed())
            .sum();

        let system_read_total = self.read_bytes_from_dead_processes
            + self
                .system_processes_iter()
                .filter_map(|process| process.data.read_bytes)
                .sum::<u64>();

        let system_write_speed = self
            .system_processes_iter()
            .filter_map(|process| process.write_speed())
            .sum();

        let system_write_total = self.write_bytes_from_dead_processes
            + self
                .system_processes_iter()
                .filter_map(|process| process.data.write_bytes)
                .sum::<u64>();

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
                read_speed: system_read_speed,
                read_total: system_read_total,
                write_speed: system_write_speed,
                write_total: system_write_total,
            },
        );
        return_map
    }

    /// Refreshes the statistics about the running applications and processes.
    pub fn refresh(&mut self, process_data: Vec<ProcessData>) {
        let newly_gathered_processes = process_data
            .into_iter()
            .map(Process::from_process_data)
            .collect::<Vec<_>>();

        let mut updated_processes = HashSet::new();

        for mut new_process in newly_gathered_processes {
            updated_processes.insert(new_process.data.pid);
            // refresh our old processes
            if let Some(old_process) = self.processes.get_mut(&new_process.data.pid) {
                old_process.cpu_time_last = old_process.data.cpu_time;
                old_process.cpu_time_last_timestamp = old_process.data.cpu_time_timestamp;
                old_process.read_bytes_last = old_process.data.read_bytes;
                old_process.read_bytes_last_timestamp = old_process.data.read_bytes_timestamp;
                old_process.write_bytes_last = old_process.data.write_bytes;
                old_process.write_bytes_last_timestamp = old_process.data.write_bytes_timestamp;
                old_process.data = new_process.data.clone();
            } else {
                // this is a new process, see if it belongs to a graphical app

                if self
                    .processes_assigned_to_apps
                    .contains(&new_process.data.pid)
                {
                    continue;
                }

                if let Some(app_id) = self.app_associated_with_process(&new_process) {
                    self.processes_assigned_to_apps.insert(new_process.data.pid);
                    self.apps
                        .get_mut(&app_id)
                        .unwrap()
                        .add_process(&mut new_process);
                }

                self.processes.insert(new_process.data.pid, new_process);
            }
        }

        // all the not-updated processes have unfortunately died, probably

        // collect the I/O stats for died app processes so an app doesn't suddenly have less total disk I/O
        self.apps.values_mut().for_each(|app| {
            let (read_dead, write_dead) = app
                .processes
                .iter()
                .filter(|pid| !updated_processes.contains(*pid)) // only dead processes
                .filter_map(|pid| self.processes.get(pid)) // ignore about non-existing processes
                .map(|process| (process.data.read_bytes, process.data.write_bytes)) // get their read_bytes and write_bytes
                .filter_map(
                    // filter out any processes whose IO stats we were not allowed to see
                    |(read_bytes, write_bytes)| match (read_bytes, write_bytes) {
                        (Some(read), Some(write)) => Some((read, write)),
                        _ => None,
                    },
                )
                .reduce(|sum, current| (sum.0 + current.0, sum.1 + current.1)) // sum them up
                .unwrap_or((0, 0)); // if there were no processes, it's 0 for both

            app.read_bytes_from_dead_processes += read_dead;
            app.write_bytes_from_dead_processes += write_dead;

            app.processes.retain(|pid| updated_processes.contains(pid));

            if !app.is_running() {
                app.read_bytes_from_dead_processes = 0;
                app.write_bytes_from_dead_processes = 0;
            }
        });

        // same as above but for system processes
        let (read_dead, write_dead) = self
            .processes
            .iter()
            .filter(|(pid, _)| {
                !self.processes_assigned_to_apps.contains(*pid) && !updated_processes.contains(*pid)
            })
            .map(|(_, process)| (process.data.read_bytes, process.data.write_bytes))
            .filter_map(
                |(read_bytes, write_bytes)| match (read_bytes, write_bytes) {
                    (Some(read), Some(write)) => Some((read, write)),
                    _ => None,
                },
            )
            .reduce(|sum, current| (sum.0 + current.0, sum.1 + current.1))
            .unwrap_or((0, 0));
        self.read_bytes_from_dead_processes += read_dead;
        self.write_bytes_from_dead_processes += write_dead;

        // remove the dead process from our process map
        self.processes
            .retain(|pid, _| updated_processes.contains(pid));

        // remove the dead process from out list of app processes
        self.processes_assigned_to_apps
            .retain(|pid| updated_processes.contains(pid));
    }

    pub fn system_processes_iter(&self) -> impl Iterator<Item = &Process> {
        self.all_processes()
            .filter(|process| !self.processes_assigned_to_apps.contains(&process.data.pid))
    }
}
