pub mod cache;
pub mod fdinfo;
pub mod gpu_usage;
pub mod npu_usage;
pub mod nvidia;
pub mod pci_slot;
pub mod procfs;
pub mod time;

use anyhow::{Context, Result};
use log::{debug, trace};
use nutype::nutype;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::str::FromStr;
use std::sync::LazyLock;

use crate::cache::ProcessDataCache;
use crate::fdinfo::{Fdinfo, collect_fdinfos, extract_gpu_usage, extract_npu_usage};
use crate::gpu_usage::{GpuIdentifier, GpuUsageStats};
use crate::npu_usage::NpuUsageStats;
use crate::pci_slot::PciSlot;
use crate::procfs::{parse_affinity, parse_memory_usage, parse_stat, parse_swap_usage, parse_uid};
use crate::time::unix_as_millis;

// we split the stat contents where the executable name ends, which is the second element
mod stat_fields {
    pub const OFFSET: usize = 2;
    pub const PARENT_PID: usize = 3 - OFFSET;
    pub const USER_CPU_TIME: usize = 13 - OFFSET;
    pub const SYSTEM_CPU_TIME: usize = 14 - OFFSET;
    pub const NICE: usize = 18 - OFFSET;
    pub const STARTTIME: usize = 21 - OFFSET;
}

static USERS_CACHE: LazyLock<HashMap<libc::uid_t, String>> = LazyLock::new(|| {
    debug!("Initializing users cache…");
    // SAFETY: uzers::all_users() is documented to be safe to call in
    // single-threaded context, we only call this once via LazyLock
    let users: HashMap<libc::uid_t, String> = unsafe { uzers::all_users() }
        .map(|user| {
            trace!("Found user {}", user.name().to_string_lossy());
            (user.uid(), user.name().to_string_lossy().into_owned())
        })
        .collect();
    debug!("Found {} users", users.len());
    users
});

static NUM_CPUS: LazyLock<usize> = LazyLock::new(num_cpus::get);

#[nutype(
    validate(less_or_equal = 19),
    validate(greater_or_equal = -20),
    derive(
        Debug,
        Default,
        Clone,
        Hash,
        PartialEq,
        Eq,
        Serialize,
        Deserialize,
        Copy,
        FromStr,
        Deref,
        TryFrom,
        Display,
        PartialOrd,
        Ord,
    ),
    default = 0
)]
pub struct Niceness(i8);

#[derive(
    Debug, Clone, Default, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum Containerization {
    #[default]
    None,
    Flatpak,
    Portable,
    Snap,
    AppImage,
}

/// All per-process data collected from procfs / sysfs / DRM / NVML.
///
/// Serialisable so it can be forwarded by `resources-processes`.
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessData {
    pub pid: libc::pid_t,
    pub parent_pid: libc::pid_t,
    pub user: String,
    pub comm: String,
    pub commandline: String,
    pub user_cpu_time: u64,
    pub system_cpu_time: u64,
    pub niceness: Niceness,
    pub affinity: Vec<bool>,
    pub memory_usage: usize,
    pub swap_usage: usize,
    pub starttime: u64, // in clock ticks, see man proc(5)!
    pub cgroup: Option<String>,
    pub containerization: Containerization,
    pub read_bytes: Option<u64>,
    pub write_bytes: Option<u64>,
    pub timestamp: u64,
    pub gpu_usage_stats: BTreeMap<GpuIdentifier, GpuUsageStats>,
    pub npu_usage_stats: BTreeMap<PciSlot, NpuUsageStats>,
    pub appimage_path: Option<String>,
}

impl ProcessData {
    pub fn all_from_procfs(proc_root: &Path, cache: &mut ProcessDataCache) -> Result<Vec<Self>> {
        cache.nvidia_refresh();

        let mut out = Vec::new();
        for entry in std::fs::read_dir(proc_root)?.flatten() {
            if entry.file_name().to_string_lossy().parse::<u32>().is_ok() {
                if let Ok(data) = Self::from_proc_entry(&entry.path(), proc_root, cache) {
                    out.push(data);
                }
            }
        }
        Ok(out)
    }

    pub fn from_proc_entry(
        proc_path: &Path,
        proc_root: &Path,
        cache: &mut ProcessDataCache,
    ) -> Result<Self> {
        let pid: libc::pid_t = proc_path
            .file_name()
            .context("proc_path terminates in ..")?
            .to_str()
            .context("non-UTF-8 directory name")?
            .parse()?;

        trace!("Inspecting process {pid}…");

        let stat_raw = read_file_trimmed(proc_path.join("stat"))?;
        let status = read_file_trimmed(proc_path.join("status"))?;
        let comm = read_file_trimmed(proc_path.join("comm"))?.replace('\n', "");
        let commandline = read_file_trimmed(proc_path.join("cmdline"))?;
        let io = read_file_trimmed(proc_path.join("io")).ok();
        let cgroup_raw = read_file_trimmed(proc_path.join("cgroup")).ok();
        let environ_raw = read_file_trimmed(proc_path.join("environ"))?;

        // stat fields
        let stat = parse_stat(&stat_raw)?;

        let parent_pid = parse_stat_field(&stat, stat_fields::PARENT_PID, "parent_pid")?;
        let user_cpu_time = parse_stat_field(&stat, stat_fields::USER_CPU_TIME, "user_cpu_time")?;
        let system_cpu_time =
            parse_stat_field(&stat, stat_fields::SYSTEM_CPU_TIME, "system_cpu_time")?;
        let nice: Niceness = parse_stat_field(&stat, stat_fields::NICE, "niceness")?;
        let starttime = parse_stat_field(&stat, stat_fields::STARTTIME, "starttime")?;

        trace!(
            "parent_pid={parent_pid} user_cpu={user_cpu_time} sys_cpu={system_cpu_time} nice={nice} start={starttime}"
        );

        // status fields
        let uid = parse_uid(&status)?;
        let user = USERS_CACHE
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| "root".to_owned());
        trace!("user of {pid} = {user}");

        let affinity = parse_affinity(&status, *NUM_CPUS);
        let memory_usage = parse_memory_usage(&status);
        let swap_usage = parse_swap_usage(&status);

        // cgroup / containerization
        let (launcher, cgroup) = cgroup_raw
            .as_deref()
            .map(procfs::sanitize)
            .unwrap_or_default();

        let environ: HashMap<String, String> = environ_raw
            .split('\0')
            .filter_map(|e| e.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let appimage_path = environ.get("APPIMAGE").cloned();

        let containerization = detect_containerization(
            &commandline,
            proc_path,
            launcher.as_deref(),
            appimage_path.is_some(),
        );
        trace!("containerization of {pid} = {containerization:?}");

        // io
        let read_bytes = io.as_deref().and_then(procfs::parse_read_bytes);
        let write_bytes = io.as_deref().and_then(procfs::parse_write_bytes);

        // GPU/NPU
        let fdinfos = collect_fdinfos(pid, proc_root, cache).unwrap_or_default();

        let mut gpu_usage_stats = accumulate_gpu_stats(&fdinfos, cache);
        gpu_usage_stats.extend(cache.nvidia_get(pid));

        let npu_usage_stats = accumulate_npu_stats(&fdinfos, cache);

        let timestamp = unix_as_millis();
        trace!("Process {pid} done");

        Ok(Self {
            pid,
            parent_pid,
            user,
            comm,
            commandline,
            user_cpu_time,
            system_cpu_time,
            niceness: nice,
            affinity,
            memory_usage,
            swap_usage,
            starttime,
            cgroup,
            containerization,
            read_bytes,
            write_bytes,
            timestamp,
            gpu_usage_stats,
            npu_usage_stats,
            appimage_path,
        })
    }
}

fn read_file_trimmed(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_owned())
        .with_context(|| format!("failed to read {}", path.display()))
}

fn parse_stat_field<T>(fields: &[&str], index: usize, name: &str) -> Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    fields
        .get(index)
        .with_context(|| format!("stat: missing field '{name}' at index {index}"))?
        .parse::<T>()
        .with_context(|| format!("stat: failed to parse '{name}'"))
}

fn detect_containerization(
    commandline: &str,
    proc_path: &Path,
    launcher: Option<&str>,
    has_appimage: bool,
) -> Containerization {
    if commandline.starts_with("/snap/") {
        Containerization::Snap
    } else if proc_path.join("root/top.kimiblock.portable").exists() || launcher == Some("portable")
    {
        Containerization::Portable
    } else if proc_path.join("root/.flatpak-info").exists() || launcher == Some("flatpak") {
        Containerization::Flatpak
    } else if has_appimage {
        Containerization::AppImage
    } else {
        Containerization::None
    }
}

fn accumulate_gpu_stats(
    fdinfos: &[Fdinfo],
    cache: &mut ProcessDataCache,
) -> BTreeMap<GpuIdentifier, GpuUsageStats> {
    let mut map: BTreeMap<GpuIdentifier, GpuUsageStats> = BTreeMap::new();
    for fdinfo in fdinfos {
        match extract_gpu_usage(fdinfo) {
            Ok((id, stats)) => {
                trace!(
                    "GPU stats from fdinfo {}/{}: {id:?}",
                    fdinfo.pid, fdinfo.fd_num
                );
                map.entry(id)
                    .and_modify(|e| *e = e.greater(&stats))
                    .or_insert(stats);
            }
            Err(_) => {
                trace!("fdinfo {}/{} is not GPU-related", fdinfo.pid, fdinfo.fd_num);
                cache.non_gpu_fdinfos_insert(fdinfo.pid, fdinfo.fd_num);
            }
        }
    }
    map
}

fn accumulate_npu_stats(
    fdinfos: &[Fdinfo],
    cache: &mut ProcessDataCache,
) -> BTreeMap<PciSlot, NpuUsageStats> {
    let mut map: BTreeMap<PciSlot, NpuUsageStats> = BTreeMap::new();
    for fdinfo in fdinfos {
        match extract_npu_usage(fdinfo) {
            Ok((slot, stats)) => {
                trace!(
                    "NPU stats from fdinfo {}/{}: {slot}",
                    fdinfo.pid, fdinfo.fd_num
                );
                map.entry(slot)
                    .and_modify(|e| *e = e.greater(&stats))
                    .or_insert(stats);
            }
            Err(_) => {
                trace!("fdinfo {}/{} is not NPU-related", fdinfo.pid, fdinfo.fd_num);
                cache.non_npu_fdinfos_insert(fdinfo.pid, fdinfo.fd_num);
            }
        }
    }
    map
}
