use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Instant,
};

use anyhow::{bail, Context, Result};
use gtk::{
    gio::{File, FileIcon, Icon, ThemedIcon},
    glib::GString,
};
use log::{debug, info};
use process_data::{pci_slot::PciSlot, Containerization, ProcessData};
use regex::Regex;

use crate::i18n::i18n;

use super::{
    boot_time,
    process::{Process, ProcessAction},
    NaNDefault,
};

// This contains executable names that are blacklisted from being recognized as applications
const DESKTOP_EXEC_BLOCKLIST: &[&str] = &["bash", "zsh", "fish", "sh", "ksh", "flatpak"];

// This contains IDs of desktop files that shouldn't be counted as applications for whatever reason
static APP_ID_BLOCKLIST: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        (
            "org.gnome.Terminal.Preferences",
            "Prevents the actual Terminal app \"org.gnome.Terminal\" from being shown",
        ),
        (
            "org.freedesktop.IBus.Panel.Extension.Gtk3",
            "Technical application",
        ),
        ("org.gnome.RemoteDesktop.Handover", "Technical application"),
        (
            "gnome-software-local-file-packagekit",
            "Technical application",
        ),
        ("snap-handle-link", "Technical application"),
    ])
});

static RE_ENV_FILTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"env\s*\S*=\S*\s*(.*)").unwrap());

static RE_FLATPAK_FILTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"flatpak run .* --command=(\S*)").unwrap());

fn format_path(path: &str) -> String {
    if path.starts_with("~/") {
        // $HOME may not include a trailing /, so we must not remove the extra trailing /
        path.replace(
            '~',
            &std::env::var("HOME").unwrap_or_else(|_| "/".to_string()),
        )
    } else {
        path.parse().unwrap()
    }
}

// Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
pub static DATA_DIRS: LazyLock<Vec<PathBuf>> = LazyLock::new(|| {
    let local_share = format_path("~/.local/share");
    let mut data_dirs: Vec<PathBuf> = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| format!("/usr/share:{local_share}"))
        .split(':')
        .map(format_path)
        .map(PathBuf::from)
        .collect();
    data_dirs.push(PathBuf::from(local_share));
    data_dirs
});

// This contains known occurrences of processes having a too distinct name from the actual app
// The HashMap is used like this:
//   Key: The name of the executable of the process
//   Value: What it should be replaced with when finding out to which app it belongs
static KNOWN_EXECUTABLE_NAME_EXCEPTIONS: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        HashMap::from([
            ("firefox-bin", "firefox"),
            ("oosplash", "libreoffice"),
            ("soffice.bin", "libreoffice"),
            ("resources-processes", "resources"),
            ("gnome-terminal-server", "gnome-terminal"),
            ("chrome", "google-chrome-stable"),
        ])
    });

static MESSAGE_LOCALES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let envs = ["LC_MESSAGES", "LANGUAGE", "LANG", "LC_ALL"];
    let mut return_vec: Vec<String> = Vec::new();

    for env in &envs {
        if let Ok(locales) = std::env::var(env) {
            // split because LANGUAGE may contain multiple languages
            for locale in locales.split(':') {
                let locale = locale.to_string();

                if !return_vec.contains(&locale) {
                    return_vec.push(locale.clone());
                }

                if let Some(no_character_encoding) = locale.split_once('.') {
                    let no_character_encoding = no_character_encoding.0.to_string();
                    if !return_vec.contains(&no_character_encoding) {
                        return_vec.push(no_character_encoding);
                    }
                }

                if let Some(no_country_code) = locale.split_once('_') {
                    let no_country_code = no_country_code.0.to_string();
                    if !return_vec.contains(&no_country_code) {
                        return_vec.push(no_country_code);
                    }
                }
            }
        }
    }

    debug!(
        "Using the following locales for app names and descriptions: {:?}",
        &return_vec
    );

    return_vec
});

#[derive(Debug, Clone, Default)]
pub struct AppsContext {
    apps: HashMap<Option<String>, App>,
    processes: HashMap<i32, Process>,
}

/// Represents an application installed on the system. It doesn't
/// have to be running (i.e. have alive processes).
#[derive(Debug, Clone)]
pub struct App {
    processes: Vec<i32>,
    pub commandline: Option<String>,
    pub executable_name: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub icon: Icon,
    pub id: Option<String>,
    pub read_bytes_from_dead_processes: u64,
    pub write_bytes_from_dead_processes: u64,
    pub containerization: Containerization,
}

impl App {
    pub fn all() -> Vec<App> {
        debug!("Detecting installed apps");

        let start = Instant::now();

        let applications_dir: Vec<_> = DATA_DIRS
            .iter()
            .map(|path| path.join("applications"))
            .collect();

        debug!(
            "Using the following directories for app detection: {:?}",
            applications_dir
        );

        let mut apps: Vec<_> = applications_dir
            .iter()
            .flat_map(|applications_path| {
                applications_path.read_dir().ok().map(|read| {
                    read.filter_map(|file_res| {
                        file_res
                            .ok()
                            .and_then(|file| Self::from_desktop_file(file.path()).ok())
                    })
                })
            })
            .flatten()
            .collect();

        let elapsed = start.elapsed();

        info!("Detected {} apps within {elapsed:.2?}", apps.len());

        apps.push(App {
            processes: Vec::new(),
            commandline: None,
            executable_name: None,
            display_name: i18n("System Processes"),
            description: None,
            icon: ThemedIcon::new("system-processes").into(),
            id: None,
            read_bytes_from_dead_processes: 0,
            write_bytes_from_dead_processes: 0,
            containerization: Containerization::None,
        });

        apps
    }

    pub fn from_desktop_file<P: AsRef<Path>>(file_path: P) -> Result<App> {
        let file_path = file_path.as_ref();

        let ini = ini::Ini::load_from_file(file_path)?;

        let desktop_entry = ini
            .section(Some("Desktop Entry"))
            .context("no desktop entry section")?;

        let id = desktop_entry
            .get("X-Flatpak") // is there a X-Flatpak section?
            .or_else(|| desktop_entry.get("X-SnapInstanceName")) // if not, maybe there is a X-SnapInstanceName
            .map(str::to_string)
            .or_else(|| {
                // if not, presume that the ID is in the file name
                Some(file_path.file_stem()?.to_string_lossy().to_string())
            })
            .context("unable to get ID of desktop file")?;

        if let Some(reason) = APP_ID_BLOCKLIST.get(id.as_str()) {
            debug!("Skipping {id} because it's blocklisted (reason: {reason})");
            bail!("{id} is blocklisted (reason: {reason})")
        }

        let exec = desktop_entry.get("Exec");
        let is_flatpak = exec
            .map(|exec| exec.starts_with("/usr/bin/flatpak run"))
            .unwrap_or_default();
        let commandline = exec
            .and_then(|exec| {
                RE_ENV_FILTER
                    .captures(exec)
                    .and_then(|captures| captures.get(1))
                    .map(|capture| capture.as_str())
                    .or(Some(exec))
            })
            .map(str::to_string);

        let executable_name = commandline.clone().map(|cmdline| {
            RE_FLATPAK_FILTER // filter flatpak stuff (e. g. from "/usr/bin/flatpak run … --command=inkscape …" to "inkscape")
                .captures(&cmdline)
                .and_then(|captures| captures.get(1))
                .map(|capture| capture.as_str().to_string())
                .unwrap_or(cmdline) // if there's no flatpak stuff, return the bare cmdline
                .split(' ') // filter any arguments (e. g. from "/usr/bin/firefox %u" to "/usr/bin/firefox")
                .nth(0)
                .unwrap_or_default()
                .split('/') // filter the executable path (e. g. from "/usr/bin/firefox" to "firefox")
                .nth_back(0)
                .unwrap_or_default()
                .to_string()
        });

        if let Some(executable_name) = &executable_name {
            if DESKTOP_EXEC_BLOCKLIST.contains(&executable_name.as_str()) {
                debug!("Skipping {id} because its executable {executable_name} blacklisted…");
                bail!("{id}'s executable {executable_name} is blacklisted")
            }
        }

        let icon = if let Some(desktop_icon) = desktop_entry.get("Icon") {
            if Path::new(&format_path(desktop_icon)).exists() {
                FileIcon::new(&File::for_path(desktop_icon)).into()
            } else {
                ThemedIcon::new(desktop_icon).into()
            }
        } else {
            ThemedIcon::new("generic-process").into()
        };

        let mut display_name_opt = None;
        let mut description_opt = None;

        for locale in MESSAGE_LOCALES.iter() {
            if let Some(name) = desktop_entry.get(format!("Name[{locale}]")) {
                display_name_opt = Some(name);
                break;
            }
        }

        for locale in MESSAGE_LOCALES.iter() {
            if let Some(comment) = desktop_entry.get(format!("Comment[{locale}]")) {
                description_opt = Some(comment);
                break;
            }
        }

        let display_name = display_name_opt
            .or_else(|| desktop_entry.get("Name"))
            .unwrap_or(&id)
            .to_string();

        let description = description_opt
            .or_else(|| desktop_entry.get("Comment"))
            .map(str::to_string);

        let is_snap = desktop_entry.get("X-SnapInstanceName").is_some();

        let containerization = if is_flatpak {
            debug!(
                "Found Flatpak app \"{display_name}\" (ID: {id}) at {} with commandline `{}` (detected executable name: {})",
                file_path.to_string_lossy(),
                commandline.as_ref().unwrap_or(&"<None>".into()),
                executable_name.as_ref().unwrap_or(&"<None>".into()),
            );
            Containerization::Flatpak
        } else if is_snap {
            debug!(
                "Found Snap app \"{display_name}\" (ID: {id}) at {} with commandline `{}` (detected executable name: {})",
                file_path.to_string_lossy(),
                commandline.as_ref().unwrap_or(&"<None>".into()),
                executable_name.as_ref().unwrap_or(&"<None>".into()),
            );
            Containerization::Snap
        } else {
            debug!(
                "Found native app \"{display_name}\" (ID: {id}) at {} with commandline `{}` (detected executable name: {})",
                file_path.to_string_lossy(),
                commandline.as_ref().unwrap_or(&"<None>".into()),
                executable_name.as_ref().unwrap_or(&"<None>".into()),
            );
            Containerization::None
        };

        let id = Some(id);

        Ok(App {
            processes: Vec::new(),
            commandline,
            executable_name,
            display_name,
            description,
            icon,
            id,
            read_bytes_from_dead_processes: 0,
            write_bytes_from_dead_processes: 0,
            containerization,
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
        apps.processes_iter()
            .filter(move |process| self.processes.contains(&process.data.pid))
    }

    pub fn processes_iter_mut<'a>(
        &'a mut self,
        apps: &'a mut AppsContext,
    ) -> impl Iterator<Item = &mut Process> {
        apps.processes_iter_mut()
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
        self.processes_iter(apps).map(Process::cpu_time_ratio).sum()
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

    #[must_use]
    pub fn gpu_usage(&self, apps: &AppsContext) -> f32 {
        self.processes_iter(apps).map(Process::gpu_usage).sum()
    }

    #[must_use]
    pub fn enc_usage(&self, apps: &AppsContext) -> f32 {
        self.processes_iter(apps).map(Process::enc_usage).sum()
    }

    #[must_use]
    pub fn dec_usage(&self, apps: &AppsContext) -> f32 {
        self.processes_iter(apps).map(Process::dec_usage).sum()
    }

    #[must_use]
    pub fn gpu_mem_usage(&self, apps: &AppsContext) -> u64 {
        self.processes_iter(apps).map(Process::gpu_mem_usage).sum()
    }

    #[must_use]
    pub fn starttime(&self, apps: &AppsContext) -> f64 {
        self.processes_iter(apps)
            .map(Process::starttime)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or_default()
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

    pub fn running_since(&self, apps: &AppsContext) -> Result<GString> {
        boot_time()
            .and_then(|boot_time| {
                boot_time
                    .add_seconds(self.starttime(apps))
                    .context("unable to add seconds to boot time")
            })
            .and_then(|time| time.format("%c").context("unable to format running_since"))
    }

    pub fn running_processes(&self) -> usize {
        self.processes.len()
    }
}

impl AppsContext {
    /// Creates a new `AppsContext` object, this operation is quite expensive
    /// so try to do it only one time during the lifetime of the program.
    /// Please call refresh() immediately after this function.
    pub fn new() -> AppsContext {
        let apps: HashMap<Option<String>, App> = App::all()
            .into_iter()
            .map(|app| (app.id.clone(), app))
            .collect();

        AppsContext {
            apps,
            processes: HashMap::new(),
        }
    }

    pub fn gpu_fraction(&self, pci_slot: PciSlot) -> f32 {
        self.processes_iter()
            .map(|process| {
                (
                    &process.data.gpu_usage_stats,
                    &process.gpu_usage_stats_last,
                    process.data.timestamp,
                    process.timestamp_last,
                )
            })
            .map(|(new, old, timestamp, timestamp_last)| {
                (
                    new.get(&pci_slot),
                    old.get(&pci_slot),
                    timestamp,
                    timestamp_last,
                )
            })
            .filter_map(|(new, old, timestamp, timestamp_last)| match (new, old) {
                (Some(new), Some(old)) => Some((new, old, timestamp, timestamp_last)),
                _ => None,
            })
            .map(|(new, old, timestamp, timestamp_last)| {
                if new.nvidia {
                    new.gfx as f32 / 100.0
                } else if old.gfx == 0 {
                    0.0
                } else {
                    ((new.gfx.saturating_sub(old.gfx) as f32)
                        / (timestamp.saturating_sub(timestamp_last) as f32))
                        .nan_default(0.0)
                        / 1_000_000.0
                }
            })
            .sum::<f32>()
            .clamp(0.0, 1.0)
    }

    pub fn encoder_fraction(&self, pci_slot: PciSlot) -> f32 {
        self.processes_iter()
            .map(|process| {
                (
                    &process.data.gpu_usage_stats,
                    &process.gpu_usage_stats_last,
                    process.data.timestamp,
                    process.timestamp_last,
                )
            })
            .map(|(new, old, timestamp, timestamp_last)| {
                (
                    new.get(&pci_slot),
                    old.get(&pci_slot),
                    timestamp,
                    timestamp_last,
                )
            })
            .filter_map(|(new, old, timestamp, timestamp_last)| match (new, old) {
                (Some(new), Some(old)) => Some((new, old, timestamp, timestamp_last)),
                _ => None,
            })
            .map(|(new, old, timestamp, timestamp_last)| {
                if new.nvidia {
                    new.enc as f32 / 100.0
                } else if old.enc == 0 {
                    0.0
                } else {
                    ((new.enc.saturating_sub(old.enc) as f32)
                        / (timestamp.saturating_sub(timestamp_last) as f32))
                        .nan_default(0.0)
                        / 1_000_000.0
                }
            })
            .sum::<f32>()
            .clamp(0.0, 1.0)
    }

    pub fn decoder_fraction(&self, pci_slot: PciSlot) -> f32 {
        self.processes_iter()
            .map(|process| {
                (
                    &process.data.gpu_usage_stats,
                    &process.gpu_usage_stats_last,
                    process.data.timestamp,
                    process.timestamp_last,
                )
            })
            .map(|(new, old, timestamp, timestamp_last)| {
                (
                    new.get(&pci_slot),
                    old.get(&pci_slot),
                    timestamp,
                    timestamp_last,
                )
            })
            .filter_map(|(new, old, timestamp, timestamp_last)| match (new, old) {
                (Some(new), Some(old)) => Some((new, old, timestamp, timestamp_last)),
                _ => None,
            })
            .map(|(new, old, timestamp, timestamp_last)| {
                if new.nvidia {
                    new.dec as f32 / 100.0
                } else if old.dec == 0 {
                    0.0
                } else {
                    ((new.dec.saturating_sub(old.dec) as f32)
                        / (timestamp.saturating_sub(timestamp_last) as f32))
                        .nan_default(0.0)
                        / 1_000_000.0
                }
            })
            .sum::<f32>()
            .clamp(0.0, 1.0)
    }

    fn app_associated_with_process(&self, process: &Process) -> Option<String> {
        // TODO: tidy this up
        // ↓ look for whether we can find an ID in the cgroup
        if let Some(app) = self
            .apps
            .get(&Some(process.data.cgroup.clone().unwrap_or_default()))
        {
            debug!(
                "Associating process {} with app \"{}\" (ID: {}) based on process cgroup matching with app ID",
                process.data.pid, app.display_name, app.id.as_deref().unwrap_or_default()
            );
            app.id.clone()
        } else if let Some(app) = self.apps.get(&Some(process.executable_path.clone())) {
            // ↑ look for whether we can find an ID in the executable path of the process
            debug!(
                "Associating process {} with app \"{}\" (ID: {}) based on process executable path matching with app ID",
                process.data.pid, app.display_name, app.id.as_deref().unwrap_or_default()
            );
            app.id.clone()
        } else if let Some(app) = self.apps.get(&Some(process.executable_name.clone())) {
            // ↑ look for whether we can find an ID in the executable name of the process
            debug!(
                "Associating process {} with app \"{}\" (ID: {}) based on process executable name matching with app ID",
                process.data.pid, app.display_name, app.id.as_deref().unwrap_or_default()
            );
            app.id.clone()
        } else {
            self.apps
                .values()
                .find(|app| {
                    // ↓ probably most expensive lookup, therefore only last resort: look if the process' commandline
                    //   can be found in the app's commandline
                    if app
                        .commandline
                        .as_ref()
                        .is_some_and(|commandline| commandline == &process.executable_path)
                    {
                        debug!(
                            "Associating process {} with app \"{}\" (ID: {}) based on process executable pathmatching with app commandline ({})",
                            process.data.pid, app.display_name, app.id.as_deref().unwrap_or_default(), process.executable_path
                        );
                        true
                    } else if app
                        .executable_name
                        .as_ref()
                        .is_some_and(|executable_name| executable_name == &process.executable_name)
                    {
                        debug!(
                            "Associating process {} with app \"{}\" (ID: {}) based on process executable name matching with app executable name ({})",
                            process.data.pid, app.display_name, app.id.as_deref().unwrap_or_default(), process.executable_name
                        );
                        true
                    } else if app
                        .executable_name
                        .as_ref()
                        .and_then(|executable_name| {
                            KNOWN_EXECUTABLE_NAME_EXCEPTIONS
                                .get(process.executable_name.as_str())
                                .map(|substituted_executable_name| {
                                    substituted_executable_name == executable_name
                                })
                        })
                        .unwrap_or(false)
                    {
                        debug!(
                            "Associating process {} with app \"{}\" (ID: {}) based on match in KNOWN_EXECUTABLE_NAME_EXCEPTIONS",
                            process.data.pid, app.display_name, app.id.as_deref().unwrap_or_default()
                        );
                        true
                    } else {
                        false
                    }
                })
                .and_then(|app| app.id.clone())
        }
    }

    pub fn get_process(&self, pid: i32) -> Option<&Process> {
        self.processes.get(&pid)
    }

    pub fn get_app(&self, id: &Option<String>) -> Option<&App> {
        self.apps.get(id)
    }

    #[must_use]
    pub fn processes_iter(&self) -> impl Iterator<Item = &Process> {
        self.processes.values()
    }

    #[must_use]
    pub fn processes_iter_mut(&mut self) -> impl Iterator<Item = &mut Process> {
        self.processes.values_mut()
    }

    pub fn apps_iter(&self) -> impl Iterator<Item = &App> {
        self.apps.values()
    }

    pub fn running_apps_iter(&self) -> impl Iterator<Item = &App> {
        self.apps_iter().filter(|app| {
            app.is_running()
                && !app
                    .id
                    .as_ref()
                    .map(|id| id.starts_with("xdg-desktop-portal"))
                    .unwrap_or_default()
        })
    }

    /// Refreshes the statistics about the running applications and processes.
    pub fn refresh(&mut self, new_process_data: Vec<ProcessData>) {
        let mut updated_processes = HashSet::new();

        for process_data in new_process_data {
            updated_processes.insert(process_data.pid);
            // refresh our old processes
            if let Some(old_process) = self.processes.get_mut(&process_data.pid) {
                old_process.cpu_time_last = old_process
                    .data
                    .user_cpu_time
                    .saturating_add(old_process.data.system_cpu_time);
                old_process.timestamp_last = old_process.data.timestamp;
                old_process.read_bytes_last = old_process.data.read_bytes;
                old_process.write_bytes_last = old_process.data.write_bytes;
                old_process.gpu_usage_stats_last = old_process.data.gpu_usage_stats.clone();

                old_process.data = process_data.clone();
            } else {
                // this is a new process, see if it belongs to a graphical app

                let mut new_process = Process::from_process_data(process_data);

                self.apps
                    .get_mut(&self.app_associated_with_process(&new_process))
                    .unwrap()
                    .add_process(&mut new_process);

                self.processes.insert(new_process.data.pid, new_process);
            }
        }

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

        // all the not-updated processes have unfortunately died, probably
        self.processes
            .retain(|pid, _| updated_processes.contains(pid));
    }
}
