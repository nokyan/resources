use anyhow::{bail, Context, Result};
use config::LIBEXECDIR;
use glob::glob;
use process_data::{Containerization, ProcessData};
use std::process::Command;

use async_std::sync::Arc;
use async_std::sync::Mutex;
use futures_util::future::join_all;
use gtk::gio::{Icon, ThemedIcon};

use crate::config;

use super::{FLATPAK_APP_PATH, FLATPAK_SPAWN, IS_FLATPAK};

/// Represents a process that can be found within procfs.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Process {
    pub data: ProcessData,
    pub executable_path: String,
    pub executable_name: String,
    pub icon: Icon,
    pub cpu_time_before: u64,
    pub cpu_time_before_timestamp: u64,
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
                return_vec.push(Self::from_process_data(process_data));
            }
        } else {
            let vec: Arc<Mutex<Vec<ProcessData>>> = Arc::new(Mutex::new(Vec::new()));

            let mut handles = vec![];
            for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
                let vec = Arc::clone(&vec);

                let handle = async_std::task::spawn(async move {
                    if let Ok(process_data) = ProcessData::try_from_path(entry).await {
                        vec.lock().await.push(process_data);
                    }
                });

                handles.push(handle);
            }
            join_all(handles).await;

            let vec = vec.lock().await;

            for process_data in vec.iter() {
                return_vec.push(Self::from_process_data(process_data.clone()));
            }
        }

        Ok(return_vec)
    }

    fn from_process_data(process_data: ProcessData) -> Self {
        let executable_path = process_data
            .commandline
            .split('\0')
            .nth(0)
            .unwrap_or_default()
            .to_string();

        let executable_name = executable_path
            .split('/')
            .nth_back(0)
            .unwrap_or_default()
            .to_string();

        Self {
            executable_path,
            executable_name,
            data: process_data,
            icon: ThemedIcon::new("generic-process").into(),
            cpu_time_before: 0,
            cpu_time_before_timestamp: 0,
        }
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
                / (self
                    .data
                    .cpu_time_timestamp
                    .saturating_sub(self.cpu_time_before_timestamp)) as f32)
                .clamp(0.0, 1.0)
        }
    }

    pub fn sanitize_cmdline<S: AsRef<str>>(cmdline: S) -> Option<String> {
        let cmdline = cmdline.as_ref();
        if cmdline.is_empty() {
            None
        } else {
            Some(cmdline.replace('\0', " "))
        }
    }
}
