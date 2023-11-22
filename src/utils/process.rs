use anyhow::{bail, Context, Result};
use config::LIBEXECDIR;
use glob::glob;
use process_data::{Containerization, ProcessData};
use std::process::Command;

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
    pub cpu_time_last: u64,
    pub cpu_time_last_timestamp: u64,
    pub read_bytes_last: Option<u64>,
    pub read_bytes_last_timestamp: Option<u64>,
    pub write_bytes_last: Option<u64>,
    pub write_bytes_last_timestamp: Option<u64>,
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
    pub read_speed: Option<f64>,
    pub read_total: Option<u64>,
    pub write_speed: Option<f64>,
    pub write_total: Option<u64>,
}

impl Process {
    /// Returns a `Vec` containing all currently running processes.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems traversing and
    /// parsing procfs
    pub fn all_data() -> Result<Vec<ProcessData>> {
        if *IS_FLATPAK {
            let proxy_path = format!(
                "{}/libexec/resources/resources-processes",
                FLATPAK_APP_PATH.as_str()
            );
            let command = Command::new(FLATPAK_SPAWN)
                .args(["--host", proxy_path.as_str()])
                .output()?;
            let output = command.stdout;
            let proxy_output: Vec<ProcessData> =
                rmp_serde::from_slice::<Vec<ProcessData>>(&output)?;

            return Ok(proxy_output);
        }

        let mut process_data = vec![];

        for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
            if let Ok(entry) = ProcessData::try_from_path(entry) {
                process_data.push(entry);
            }
        }

        Ok(process_data)
    }

    pub fn from_process_data(process_data: ProcessData) -> Self {
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

        let (read_bytes_last, read_bytes_last_timestamp) = if process_data.read_bytes.is_some() {
            (Some(0), Some(0))
        } else {
            (None, None)
        };

        let (write_bytes_last, write_bytes_last_timestamp) = if process_data.write_bytes.is_some() {
            (Some(0), Some(0))
        } else {
            (None, None)
        };

        Self {
            executable_path,
            executable_name,
            data: process_data,
            icon: ThemedIcon::new("generic-process").into(),
            cpu_time_last: 0,
            cpu_time_last_timestamp: 0,
            read_bytes_last,
            read_bytes_last_timestamp,
            write_bytes_last,
            write_bytes_last_timestamp,
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
        if self.cpu_time_last == 0 {
            0.0
        } else {
            (self.data.cpu_time.saturating_sub(self.cpu_time_last) as f32
                / (self
                    .data
                    .cpu_time_timestamp
                    .saturating_sub(self.cpu_time_last_timestamp)) as f32)
                .clamp(0.0, 1.0)
        }
    }

    #[must_use]
    pub fn read_speed(&self) -> Option<f64> {
        if let (
            Some(read_bytes),
            Some(read_bytes_timestamp),
            Some(read_bytes_last),
            Some(read_bytes_last_timestamp),
        ) = (
            self.data.read_bytes,
            self.data.read_bytes_timestamp,
            self.read_bytes_last,
            self.read_bytes_last_timestamp,
        ) {
            if read_bytes_last_timestamp == 0 {
                Some(0.0)
            } else {
                let bytes_delta = read_bytes.saturating_sub(read_bytes_last) as f64;
                let time_delta =
                    read_bytes_timestamp.saturating_sub(read_bytes_last_timestamp) as f64;
                Some((bytes_delta / time_delta) * 1000.0)
            }
        } else {
            None
        }
    }

    #[must_use]
    pub fn write_speed(&self) -> Option<f64> {
        if let (
            Some(write_bytes),
            Some(write_bytes_timestamp),
            Some(write_bytes_last),
            Some(write_bytes_last_timestamp),
        ) = (
            self.data.write_bytes,
            self.data.write_bytes_timestamp,
            self.write_bytes_last,
            self.write_bytes_last_timestamp,
        ) {
            if write_bytes_last_timestamp == 0 {
                Some(0.0)
            } else {
                let bytes_delta = write_bytes.saturating_sub(write_bytes_last) as f64;
                let time_delta =
                    write_bytes_timestamp.saturating_sub(write_bytes_last_timestamp) as f64;
                Some((bytes_delta / time_delta) * 1000.0)
            }
        } else {
            None
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
