pub mod pci_slot;

use anyhow::{anyhow, bail, Context, Result};
use glob::glob;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::struct_wrappers::device::{ProcessInfo, ProcessUtilizationSample};
use nvml_wrapper::{Device, Nvml};
use once_cell::sync::Lazy;
use pci_slot::PciSlot;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::os::linux::fs::MetadataExt;
use std::str::FromStr;
use std::sync::Arc;
use std::{path::PathBuf, time::SystemTime};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinSet;

static PAGESIZE: Lazy<usize> = Lazy::new(sysconf::pagesize);

static UID_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"Uid:\s*(\d+)").unwrap());

static IO_READ_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"read_bytes:\s*(\d+)").unwrap());

static IO_WRITE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"write_bytes:\s*(\d+)").unwrap());

static DRM_PDEV_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-pdev:\s*(\d{4}:\d{2}:\d{2}.\d)").unwrap());

static DRM_CLIENT_ID_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-client-id:\s*(\d+)").unwrap());

static DRM_ENGINE_GFX_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-gfx:\s*(\d+) ns").unwrap());

static DRM_ENGINE_ENC_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-enc:\s*(\d+) ns").unwrap());

static DRM_ENGINE_DEC_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-dec:\s*(\d+) ns").unwrap());

static DRM_MEMORY_VRAM_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-memory-vram:\s*(\d+) KiB").unwrap());

static DRM_MEMORY_GTT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-memory-gtt:\s*(\d+) KiB").unwrap());

static NVML: Lazy<Result<Nvml, NvmlError>> = Lazy::new(Nvml::init);

static NVML_DEVICES: Lazy<Vec<(PciSlot, Device)>> = Lazy::new(|| {
    if let Ok(nvml) = NVML.as_ref() {
        let device_count = nvml.device_count().unwrap_or(0);
        let mut return_vec = Vec::with_capacity(device_count as usize);
        for i in 0..device_count {
            if let Ok(gpu) = nvml.device_by_index(i) {
                if let Ok(pci_slot) = gpu.pci_info().map(|pci_info| pci_info.bus_id) {
                    // the PCI Slot ID given by NVML has 4 additional leading zeroes, remove those for consistency
                    let pci_slot = PciSlot::from_str(&pci_slot[4..pci_slot.len()]).unwrap();
                    return_vec.push((pci_slot, gpu));
                }
            }
        }
        return_vec
    } else {
        Vec::new()
    }
});

static NVIDIA_PROCESSES_STATS: Lazy<RwLock<HashMap<PciSlot, Vec<ProcessUtilizationSample>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static NVIDIA_PROCESS_INFOS: Lazy<RwLock<HashMap<PciSlot, Vec<ProcessInfo>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum Containerization {
    #[default]
    None,
    Flatpak,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub struct GpuUsageStats {
    pub gfx: u64,
    pub gfx_timestamp: u64,
    pub mem: u64,
    pub enc: u64,
    pub enc_timestamp: u64,
    pub dec: u64,
    pub dec_timestamp: u64,
    pub nvidia: bool,
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
    pub read_bytes: Option<u64>,
    pub read_bytes_timestamp: Option<u64>,
    pub write_bytes: Option<u64>,
    pub write_bytes_timestamp: Option<u64>,
    /// Key: PCI Slot ID of the GPU
    pub gpu_usage_stats: BTreeMap<PciSlot, GpuUsageStats>,
}

impl ProcessData {
    pub fn unix_as_millis() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
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

    async fn get_uid(proc_path: &PathBuf) -> Result<u32> {
        let status = tokio::fs::read_to_string(proc_path.join("status")).await?;
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

    pub async fn update_nvidia_stats() {
        {
            let mut stats = NVIDIA_PROCESSES_STATS.write().await;
            stats.clear();
            stats.extend(Self::nvidia_process_stats());
        }
        {
            let mut infos = NVIDIA_PROCESS_INFOS.write().await;
            infos.clear();
            infos.extend(Self::nvidia_process_infos());
        }
    }

    pub async fn all_process_data() -> Result<Vec<Self>> {
        Self::update_nvidia_stats().await;

        let mut tasks = JoinSet::new();

        for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
            tasks.spawn(async move { ProcessData::try_from_path(entry).await });
        }

        let mut process_data = Vec::with_capacity(tasks.len());

        while let Some(task) = tasks.join_next().await {
            if let Ok(data) = task.unwrap() {
                process_data.push(data);
            }
        }

        Ok(process_data)
    }

    pub async fn try_from_path(proc_path: PathBuf) -> Result<Self> {
        // Stat
        let shared_proc_path = Arc::new(proc_path.clone());
        let stat = tokio::task::spawn(async move {
            tokio::fs::read_to_string(shared_proc_path.join("stat")).await
        });

        // Statm
        let shared_proc_path = Arc::new(proc_path.clone());
        let statm = tokio::task::spawn(async move {
            tokio::fs::read_to_string(shared_proc_path.join("statm")).await
        });

        // Comm
        let shared_proc_path = Arc::new(proc_path.clone());
        let comm = tokio::task::spawn(async move {
            tokio::fs::read_to_string(shared_proc_path.join("comm")).await
        });

        // Cmdline
        let shared_proc_path = Arc::new(proc_path.clone());
        let commandline = tokio::task::spawn(async move {
            tokio::fs::read_to_string(shared_proc_path.join("cmdline")).await
        });

        // Cgroup
        let shared_proc_path = Arc::new(proc_path.clone());
        let cgroup = tokio::task::spawn(async move {
            tokio::fs::read_to_string(shared_proc_path.join("cgroup")).await
        });

        // IO
        let shared_proc_path = Arc::new(proc_path.clone());
        let io = tokio::task::spawn(async move {
            tokio::fs::read_to_string(shared_proc_path.join("io")).await
        });

        let stat = stat.await??;
        let statm = statm.await??;
        let comm = comm.await??;
        let commandline = commandline.await??;
        let cgroup = cgroup.await??;

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

        let (mut read_bytes, mut read_bytes_timestamp, mut write_bytes, mut write_bytes_timestamp) =
            (None, None, None, None);

        if let Ok(io) = io.await? {
            read_bytes = IO_READ_REGEX
                .captures(&io)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok());

            read_bytes_timestamp = if read_bytes.is_some() {
                Some(
                    SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)?
                        .as_millis() as u64,
                )
            } else {
                None
            };

            write_bytes = IO_WRITE_REGEX
                .captures(&io)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok());

            write_bytes_timestamp = if write_bytes.is_some() {
                Some(
                    SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)?
                        .as_millis() as u64,
                )
            } else {
                None
            };
        }

        let gpu_usage_stats = Self::gpu_usage_stats(&proc_path, pid).await;

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
            gpu_usage_stats,
        })
    }

    async fn gpu_usage_stats(proc_path: &PathBuf, pid: i32) -> BTreeMap<PciSlot, GpuUsageStats> {
        let nvidia_stats = Self::nvidia_gpu_stats_all(pid).await.unwrap_or_default();
        let mut other_stats = Self::other_gpu_usage_stats(proc_path, pid)
            .await
            .unwrap_or_default();
        other_stats.extend(nvidia_stats.into_iter());
        other_stats
    }

    async fn other_gpu_usage_stats(
        proc_path: &PathBuf,
        pid: i32,
    ) -> Result<BTreeMap<PciSlot, GpuUsageStats>> {
        let fdinfo_path = proc_path.join("fdinfo");
        let mut dir = tokio::fs::read_dir(fdinfo_path).await?;

        let seen_fds = Arc::new(Mutex::new(HashSet::new()));

        let mut tasks = JoinSet::new();

        while let Ok(Some(entry)) = dir.next_entry().await {
            let fd_path = entry.path();

            let seen_fds = Arc::clone(&seen_fds);

            tasks.spawn(async move {
                let mut file = File::open(&fd_path).await?;
                let metadata = file.metadata().await?;

                // if our fd is 0, 1 or 2 it's probably just a std stream so skip it
                let fd_num = fd_path
                    .file_name()
                    .and_then(|osstr| osstr.to_str())
                    .context("this fd has no filename?")?
                    .parse::<usize>()?;
                if fd_num <= 2 {
                    std::mem::drop(file);
                    bail!("stdin/stdout/stderr")
                }

                if !metadata.is_file() {
                    std::mem::drop(file);
                    bail!("not a file");
                }

                // Adapted from nvtop's `is_drm_fd()`
                // https://github.com/Syllo/nvtop/blob/master/src/extract_processinfo_fdinfo.c
                // FIXME: Recognizes every fd as non-DRM
                let major = unsafe { libc::major(metadata.st_rdev()) };
                if (metadata.st_mode() & libc::S_IFMT) != libc::S_IFCHR || major != 226 {
                    std::mem::drop(file);
                    bail!("not a DRM file")
                }

                // Adapted from nvtop's `processinfo_sweep_fdinfos()`
                // https://github.com/Syllo/nvtop/blob/master/src/extract_processinfo_fdinfo.c
                // if we've already seen the file this fd refers to, skip
                let not_unique = seen_fds.lock().await.iter().any(|seen_fd| unsafe {
                    syscalls::syscall!(syscalls::Sysno::kcmp, pid, pid, 0, fd_num, *seen_fd)
                        .unwrap_or(0)
                        == 0
                });
                if not_unique {
                    std::mem::drop(file);
                    bail!("not unique fd")
                }

                seen_fds.lock().await.insert(fd_num);

                Self::read_fdinfo(&mut file, metadata.len() as usize).await
            });
        }

        let mut return_map = BTreeMap::new();

        while let Some(stats) = tasks.join_next().await {
            if let Ok(stats) = stats.unwrap() {
                return_map
                    .entry(stats.0)
                    .and_modify(|existing_value: &mut GpuUsageStats| {
                        if stats.1.gfx > existing_value.gfx {
                            existing_value.gfx = stats.1.gfx;
                            existing_value.gfx_timestamp = stats.1.gfx_timestamp;
                        }
                        if stats.1.dec > existing_value.dec {
                            existing_value.dec = stats.1.dec;
                            existing_value.dec_timestamp = stats.1.dec_timestamp;
                        }
                        if stats.1.enc > existing_value.enc {
                            existing_value.enc = stats.1.enc;
                            existing_value.enc_timestamp = stats.1.enc_timestamp;
                        }
                        if stats.1.mem > existing_value.mem {
                            existing_value.mem = stats.1.mem;
                        }
                    })
                    .or_insert(stats.1);
            }
        }

        Ok(return_map)
    }

    async fn read_fdinfo(
        fdinfo_file: &mut File,
        file_size: usize,
    ) -> Result<(PciSlot, GpuUsageStats, i64)> {
        let mut content = String::with_capacity(file_size);
        fdinfo_file.read_to_string(&mut content).await?;
        fdinfo_file.flush().await?;

        let pci_slot = DRM_PDEV_REGEX
            .captures(&content)
            .and_then(|captures| captures.get(1))
            .map(|capture| PciSlot::from_str(capture.as_str()));

        let client_id = DRM_CLIENT_ID_REGEX
            .captures(&content)
            .and_then(|captures| captures.get(1))
            .and_then(|capture| capture.as_str().parse::<i64>().ok());

        if let (Some(Ok(pci_slot)), Some(client_id)) = (pci_slot, client_id) {
            let gfx = DRM_ENGINE_GFX_REGEX
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default();

            let enc = DRM_ENGINE_ENC_REGEX
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default();

            let dec = DRM_ENGINE_DEC_REGEX
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default();

            let vram = DRM_MEMORY_VRAM_REGEX
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default()
                * 1024;

            let gtt = DRM_MEMORY_GTT_REGEX
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default()
                * 1024;

            let stats = GpuUsageStats {
                gfx,
                gfx_timestamp: Self::unix_as_millis(),
                mem: vram.saturating_add(gtt),
                enc,
                enc_timestamp: Self::unix_as_millis(),
                dec,
                dec_timestamp: Self::unix_as_millis(),
                nvidia: false,
            };

            return Ok((pci_slot, stats, client_id));
        }

        bail!("unable to find gpu information in this fdinfo");
    }

    async fn nvidia_gpu_stats_all(pid: i32) -> Result<BTreeMap<PciSlot, GpuUsageStats>> {
        let mut return_map = BTreeMap::new();

        for (pci_slot, _) in NVML_DEVICES.iter() {
            if let Ok(stats) = Self::nvidia_gpu_stats(pid, *pci_slot).await {
                return_map.insert(pci_slot.to_owned(), stats);
            }
        }

        Ok(return_map)
    }

    async fn nvidia_gpu_stats(pid: i32, pci_slot: PciSlot) -> Result<GpuUsageStats> {
        let this_process_stats = NVIDIA_PROCESSES_STATS
            .read()
            .await
            .get(&pci_slot)
            .context("couldn't find GPU with this PCI slot")?
            .iter()
            .filter(|process| process.pid == pid as u32)
            .map(|stats| (stats.sm_util, stats.enc_util, stats.dec_util))
            .reduce(|acc, curr| (acc.0 + curr.0, acc.1 + curr.1, acc.2 + curr.2));

        let this_process_mem_stats: u64 = NVIDIA_PROCESS_INFOS
            .read()
            .await
            .get(&pci_slot)
            .context("couldn't find GPU with this PCI slot")?
            .iter()
            .filter(|process| process.pid == pid as u32)
            .map(|stats| match stats.used_gpu_memory {
                UsedGpuMemory::Unavailable => 0,
                UsedGpuMemory::Used(bytes) => bytes,
            })
            .sum();

        let gpu_stats = GpuUsageStats {
            gfx: this_process_stats.unwrap_or_default().0 as u64,
            gfx_timestamp: Self::unix_as_millis(),
            mem: this_process_mem_stats,
            enc: this_process_stats.unwrap_or_default().1 as u64,
            enc_timestamp: Self::unix_as_millis(),
            dec: this_process_stats.unwrap_or_default().2 as u64,
            dec_timestamp: Self::unix_as_millis(),
            nvidia: true,
        };
        Ok(gpu_stats)
    }

    fn nvidia_process_infos() -> HashMap<PciSlot, Vec<ProcessInfo>> {
        let mut return_map = HashMap::new();

        for (pci_slot, gpu) in NVML_DEVICES.iter() {
            let mut comp_gfx_stats = gpu.running_graphics_processes().unwrap_or_default();
            comp_gfx_stats.extend(gpu.running_compute_processes().unwrap_or_default());

            return_map.insert(pci_slot.to_owned(), comp_gfx_stats);
        }

        return_map
    }

    fn nvidia_process_stats() -> HashMap<PciSlot, Vec<ProcessUtilizationSample>> {
        let mut return_map = HashMap::new();

        for (pci_slot, gpu) in NVML_DEVICES.iter() {
            return_map.insert(
                pci_slot.to_owned(),
                gpu.process_utilization_stats(ProcessData::unix_as_millis() * 1000 - 5_000_000)
                    .unwrap_or_default(),
            );
        }

        return_map
    }
}
