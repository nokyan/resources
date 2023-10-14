use std::{path::PathBuf, time::SystemTime};

use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static PAGESIZE: Lazy<usize> = Lazy::new(sysconf::pagesize);

static UID_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"Uid:\s*(\d+)").unwrap());

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum Containerization {
    #[default]
    None,
    Flatpak,
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

    async fn get_uid(proc_path: &PathBuf) -> Result<u32> {
        let status = async_std::fs::read_to_string(proc_path.join("status")).await?;
        if let Some(captures) = UID_REGEX.captures(&status) {
            let first_num_str = captures.get(1).context("no uid found")?;
            first_num_str
                .as_str()
                .parse::<u32>()
                .context("couldn't parse uid in /status")
        } else {
            Ok(0)
        }
    }

    pub async fn try_from_path(proc_path: PathBuf) -> Result<Self> {
        let stat: Vec<String> = async_std::fs::read_to_string(proc_path.join("stat"))
            .await?
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect();

        let statm: Vec<String> = async_std::fs::read_to_string(proc_path.join("statm"))
            .await?
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect();

        let containerization = match &proc_path.join("root").join(".flatpak-info").exists() {
            true => Containerization::Flatpak,
            false => Containerization::None,
        };

        Ok(Self {
            pid: proc_path
                .file_name()
                .ok_or_else(|| anyhow!(""))?
                .to_str()
                .ok_or_else(|| anyhow!(""))?
                .parse()?,
            uid: Self::get_uid(&proc_path).await?,
            comm: async_std::fs::read_to_string(proc_path.join("comm"))
                .await
                .map(|s| s.replace('\n', ""))?,
            commandline: async_std::fs::read_to_string(proc_path.join("cmdline")).await?,
            cpu_time: stat[13].parse::<u64>()? + stat[14].parse::<u64>()?,
            cpu_time_timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis() as u64,
            memory_usage: (statm[1].parse::<usize>()? - statm[2].parse::<usize>()?) * *PAGESIZE,
            cgroup: Self::sanitize_cgroup(
                async_std::fs::read_to_string(proc_path.join("cgroup")).await?,
            ),
            proc_path,
            containerization,
        })
    }
}
