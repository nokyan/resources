use anyhow::{anyhow, Context, Result};
use async_std::sync::Arc;
use nparse::KVStrToJson;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::SystemTime};

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
    pub read_bytes: u64,
    pub read_bytes_timestamp: u64,
    pub write_bytes: u64,
    pub write_bytes_timestamp: u64,
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
        // Stat
        let shared_proc_path = Arc::new(proc_path.clone());
        let stat = async_std::task::spawn(async move {
            async_std::fs::read_to_string(shared_proc_path.join("stat")).await
        });

        // Statm
        let shared_proc_path = Arc::new(proc_path.clone());
        let statm = async_std::task::spawn(async move {
            async_std::fs::read_to_string(shared_proc_path.join("statm")).await
        });

        // Comm
        let shared_proc_path = Arc::new(proc_path.clone());
        let comm = async_std::task::spawn(async move {
            async_std::fs::read_to_string(shared_proc_path.join("comm")).await
        });

        // Cmdline
        let shared_proc_path = Arc::new(proc_path.clone());
        let commandline = async_std::task::spawn(async move {
            async_std::fs::read_to_string(shared_proc_path.join("cmdline")).await
        });

        // Cgroup
        let shared_proc_path = Arc::new(proc_path.clone());
        let cgroup = async_std::task::spawn(async move {
            async_std::fs::read_to_string(shared_proc_path.join("cgroup")).await
        });

        // IO
        let shared_proc_path = Arc::new(proc_path.clone());
        let io = async_std::task::spawn(async move {
            async_std::fs::read_to_string(shared_proc_path.join("io")).await
        });

        let stat = stat.await?;
        let statm = statm.await?;
        let comm = comm.await?;
        let commandline = commandline.await?;
        let cgroup = cgroup.await?;

        let pid = proc_path
            .file_name()
            .ok_or_else(|| anyhow!(""))?
            .to_str()
            .ok_or_else(|| anyhow!(""))?
            .parse()?;

        let uid = Self::get_uid(&proc_path).await?;

        let stat = stat
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();

        let statm = statm
            .split(' ')
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();

        let comm = comm.replace('\n', "");

        let cpu_time = stat[13].parse::<u64>()? + stat[14].parse::<u64>()?;

        let cpu_time_timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis() as u64;

        let memory_usage = (statm[1].parse::<usize>()? - statm[2].parse::<usize>()?) * *PAGESIZE;

        let cgroup = Self::sanitize_cgroup(cgroup);

        let containerization = match &proc_path.join("root").join(".flatpak-info").exists() {
            true => Containerization::Flatpak,
            false => Containerization::None,
        };

        let io = io.await?.kv_str_to_json().ok();

        let read_bytes = io
            .as_ref()
            .and_then(|kv| {
                kv.as_object().and_then(|obj| {
                    obj.get("read_bytes")
                        .and_then(|val| val.as_str().and_then(|s| s.parse().ok()))
                })
            })
            .unwrap_or(0);

        let read_bytes_timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis() as u64;

        let write_bytes = io
            .and_then(|kv| {
                kv.as_object().and_then(|obj| {
                    obj.get("write_bytes")
                        .and_then(|val| val.as_str().and_then(|s| s.parse().ok()))
                })
            })
            .unwrap_or(0);

        let write_bytes_timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis() as u64;

        Ok(Self {
            pid,
            uid,
            comm,
            commandline,
            cpu_time,
            cpu_time_timestamp,
            memory_usage,
            cgroup,
            proc_path,
            containerization,
            read_bytes,
            read_bytes_timestamp,
            write_bytes,
            write_bytes_timestamp,
        })
    }
}
