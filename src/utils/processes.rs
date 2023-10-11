use anyhow::{anyhow, bail, Context, Result};
use config::LIBEXECDIR;
use glob::glob;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command, sync::OnceLock, time::SystemTime};

use gtk::gio::{Icon, ThemedIcon};

use crate::config;

use super::{FLATPAK_APP_PATH, FLATPAK_SPAWN, IS_FLATPAK};

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

// TODO: Better name?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    TERM,
    STOP,
    KILL,
    CONT,
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

impl Process {
    /// Returns a `Vec` containing all currently running processes.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems traversing and
    /// parsing procfs
    pub async fn all() -> Result<Vec<Self>> {
        let mut return_vec = Vec::new();

        if *IS_FLATPAK {
            let proxy_path = format!(
                "{}/libexec/resources/resources-processes",
                FLATPAK_APP_PATH.as_str()
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

        let kill_path = if *IS_FLATPAK {
            format!(
                "{}/libexec/resources/resources-kill",
                FLATPAK_APP_PATH.as_str()
            )
        } else {
            format!("{LIBEXECDIR}/resources-kill")
        };

        let status_code = if *IS_FLATPAK {
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
        let status_code = if *IS_FLATPAK {
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
