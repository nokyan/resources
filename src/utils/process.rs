use anyhow::{bail, Context, Result};
use config::LIBEXECDIR;
use log::debug;
use once_cell::sync::Lazy;
use process_data::{pci_slot::PciSlot, Containerization, GpuUsageStats, ProcessData};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    process::{ChildStdin, ChildStdout, Command, Stdio},
    sync::Mutex,
};
use strum_macros::Display;

use gtk::{
    gio::{Icon, ThemedIcon},
    glib::GString,
};

use crate::config;

use super::{NaNDefault, FLATPAK_APP_PATH, FLATPAK_SPAWN, IS_FLATPAK, NUM_CPUS, TICK_RATE};

static OTHER_PROCESS: Lazy<Mutex<(ChildStdin, ChildStdout)>> = Lazy::new(|| {
    let proxy_path = if *IS_FLATPAK {
        format!(
            "{}/libexec/resources/resources-processes",
            FLATPAK_APP_PATH.as_str()
        )
    } else {
        format!("{LIBEXECDIR}/resources-processes")
    };

    let child = if *IS_FLATPAK {
        Command::new(FLATPAK_SPAWN)
            .args(["--host", proxy_path.as_str()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    } else {
        Command::new(proxy_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };

    let stdin = child.stdin.unwrap();
    let stdout = child.stdout.unwrap();

    Mutex::new((stdin, stdout))
});

/// Represents a process that can be found within procfs.
#[derive(Debug, Clone, PartialEq)]
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
    pub gpu_usage_stats_last: BTreeMap<PciSlot, GpuUsageStats>,
}

// TODO: Better name?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
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
    pub running_since: GString,
    pub cgroup: Option<String>,
    pub read_speed: Option<f64>,
    pub read_total: Option<u64>,
    pub write_speed: Option<f64>,
    pub write_total: Option<u64>,
    pub gpu_usage: f32,
    pub enc_usage: f32,
    pub dec_usage: f32,
    pub gpu_mem_usage: u64,
}

impl Process {
    /// Returns a `Vec` containing all currently running processes.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems traversing and
    /// parsing procfs
    pub fn all_data() -> Result<Vec<ProcessData>> {
        let output = {
            let mut process = OTHER_PROCESS.lock().unwrap();
            let _ = process.0.write_all(&[b'\n']);
            let _ = process.0.flush();

            let mut len_bytes = [0_u8; (usize::BITS / 8) as usize];

            process.1.read_exact(&mut len_bytes)?;

            let len = usize::from_le_bytes(len_bytes);

            let mut output_bytes = vec![0; len];
            process.1.read_exact(&mut output_bytes)?;

            output_bytes
        };

        rmp_serde::from_slice::<Vec<ProcessData>>(&output)
            .context("error decoding resources-processes' output")
    }

    pub fn from_process_data(process_data: ProcessData) -> Self {
        let executable_path = process_data
            .commandline
            .split('\0')
            .nth(0)
            .and_then(|nul_split| nul_split.split(" --").nth(0)) // chromium (and thus everything based on it) doesn't use \0 as delimiter
            .unwrap_or(&process_data.commandline)
            .to_string();

        let executable_name = executable_path
            .split('/')
            .nth_back(0)
            .unwrap_or(&process_data.commandline)
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
            gpu_usage_stats_last: Default::default(),
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
            debug!("Successfully {action}ed {}", self.data.pid);
            Ok(())
        } else if status_code == 1 {
            // 1 := no permissions
            debug!(
                "No permissions to {action} {}, attempting pkexec",
                self.data.pid
            );
            self.pkexec_execute_process_action(action_str, &kill_path)
        } else {
            bail!(
                "couldn't {action} {} due to unknown reasons, status code: {}",
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
            debug!(
                "Successfully {action}ed {} with elevated privileges",
                self.data.pid
            );
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
            let delta_cpu_time =
                self.data.cpu_time.saturating_sub(self.cpu_time_last) as f32 * 1000.0;
            let delta_time = self
                .data
                .cpu_time_timestamp
                .saturating_sub(self.cpu_time_last_timestamp);

            delta_cpu_time / (delta_time * *TICK_RATE as u64 * *NUM_CPUS as u64) as f32
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

    #[must_use]
    pub fn gpu_usage(&self) -> f32 {
        let mut returned_gpu_usage = 0.0;
        for (gpu, usage) in self.data.gpu_usage_stats.iter() {
            if let Some(old_usage) = self.gpu_usage_stats_last.get(gpu) {
                let this_gpu_usage = if usage.nvidia {
                    usage.gfx as f32 / 100.0
                } else if old_usage.gfx == 0 {
                    0.0
                } else {
                    ((usage.gfx.saturating_sub(old_usage.gfx) as f32)
                        / (usage.gfx_timestamp.saturating_sub(old_usage.gfx_timestamp) as f32)
                            .nan_default(0.0))
                        / 1_000_000.0
                };

                if this_gpu_usage > returned_gpu_usage {
                    returned_gpu_usage = this_gpu_usage;
                }
            }
        }

        returned_gpu_usage
    }

    #[must_use]
    pub fn enc_usage(&self) -> f32 {
        let mut returned_gpu_usage = 0.0;
        for (gpu, usage) in self.data.gpu_usage_stats.iter() {
            if let Some(old_usage) = self.gpu_usage_stats_last.get(gpu) {
                let this_gpu_usage = if usage.nvidia {
                    usage.enc as f32 / 100.0
                } else if old_usage.enc == 0 {
                    0.0
                } else {
                    ((usage.enc.saturating_sub(old_usage.enc) as f32)
                        / (usage.enc_timestamp.saturating_sub(old_usage.enc_timestamp) as f32)
                            .nan_default(0.0))
                        / 1_000_000.0
                };

                if this_gpu_usage > returned_gpu_usage {
                    returned_gpu_usage = this_gpu_usage;
                }
            }
        }

        returned_gpu_usage
    }

    #[must_use]
    pub fn dec_usage(&self) -> f32 {
        let mut returned_gpu_usage = 0.0;
        for (gpu, usage) in self.data.gpu_usage_stats.iter() {
            if let Some(old_usage) = self.gpu_usage_stats_last.get(gpu) {
                let this_gpu_usage = if usage.nvidia {
                    usage.dec as f32 / 100.0
                } else if old_usage.dec == 0 {
                    0.0
                } else {
                    ((usage.dec.saturating_sub(old_usage.dec) as f32)
                        / (usage.dec_timestamp.saturating_sub(old_usage.dec_timestamp) as f32)
                            .nan_default(0.0))
                        / 1_000_000.0
                };

                if this_gpu_usage > returned_gpu_usage {
                    returned_gpu_usage = this_gpu_usage;
                }
            }
        }

        returned_gpu_usage
    }

    #[must_use]
    pub fn gpu_mem_usage(&self) -> u64 {
        self.data
            .gpu_usage_stats
            .values()
            .map(|stats| stats.mem)
            .sum()
    }

    #[must_use]
    pub fn starttime(&self) -> u64 {
        self.data.starttime / *TICK_RATE as u64
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
