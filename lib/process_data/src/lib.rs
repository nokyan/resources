pub mod pci_slot;

use anyhow::{bail, Context, Result};
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
use std::fs::File;
use std::io::{Read, Write};
use std::os::linux::fs::MetadataExt;
use std::str::FromStr;
use std::sync::RwLock;
use std::{path::PathBuf, time::SystemTime};

static PAGESIZE: Lazy<usize> = Lazy::new(sysconf::pagesize);

static RE_UID: Lazy<Regex> = Lazy::new(|| Regex::new(r"Uid:\s*(\d+)").unwrap());

static RE_IO_READ: Lazy<Regex> = Lazy::new(|| Regex::new(r"read_bytes:\s*(\d+)").unwrap());

static RE_IO_WRITE: Lazy<Regex> = Lazy::new(|| Regex::new(r"write_bytes:\s*(\d+)").unwrap());

static RE_DRM_PDEV: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"drm-pdev:\s*([0-9A-Fa-f]{4}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}\.[0-9A-Fa-f])").unwrap()
});

static RE_DRM_CLIENT_ID: Lazy<Regex> = Lazy::new(|| Regex::new(r"drm-client-id:\s*(\d+)").unwrap());

// AMD only
static RE_DRM_ENGINE_GFX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-gfx:\s*(\d+) ns").unwrap());

// AMD only
static RE_DRM_ENGINE_COMPUTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-compute:\s*(\d+) ns").unwrap());

// AMD only
static RE_DRM_ENGINE_ENC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-enc:\s*(\d+) ns").unwrap());

// AMD only
static RE_DRM_ENGINE_DEC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-dec:\s*(\d+) ns").unwrap());

// AMD only
static RE_DRM_MEMORY_VRAM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-memory-vram:\s*(\d+) KiB").unwrap());

// AMD only
static RE_DRM_MEMORY_GTT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-memory-gtt:\s*(\d+) KiB").unwrap());

// Intel only
static RE_DRM_ENGINE_RENDER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-render:\s*(\d+) ns").unwrap());

// Intel only
static RE_DRM_ENGINE_VIDEO: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"drm-engine-video:\s*(\d+) ns").unwrap());

static NVML: Lazy<Result<Nvml, NvmlError>> = Lazy::new(Nvml::init);

static NVML_DEVICES: Lazy<Vec<(PciSlot, Device)>> = Lazy::new(|| {
    if let Ok(nvml) = NVML.as_ref() {
        let device_count = nvml.device_count().unwrap_or(0);
        let mut return_vec = Vec::with_capacity(device_count as usize);
        for i in 0..device_count {
            if let Ok(gpu) = nvml.device_by_index(i) {
                if let Ok(pci_slot) = gpu.pci_info().map(|pci_info| pci_info.bus_id) {
                    let pci_slot = PciSlot::from_str(&pci_slot).unwrap();
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
    Snap,
}

/// Represents GPU usage statistics per-process. Depending on the GPU manufacturer (which should be determined in
/// Resources itself), these numbers need to interpreted differently
///
/// AMD (default): gfx, enc and dec are nanoseconds spent for that process
///
/// Nvidia: Process info is gathered through NVML, thus gfx, enc and dec are percentages from 0-100 (timestamps
/// are irrelevant, nvidia bool is set to true)
///
/// Intel: enc and dec are not separated, both are accumulated in enc, also mem is always going to be 0
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub struct GpuUsageStats {
    pub gfx: u64,
    pub mem: u64,
    pub enc: u64,
    pub dec: u64,
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
    pub user_cpu_time: u64,
    pub system_cpu_time: u64,
    pub cpu_time_timestamp: u64,
    pub memory_usage: usize,
    pub starttime: u64, // in clock ticks, see man proc(5)!
    pub cgroup: Option<String>,
    pub containerization: Containerization,
    pub read_bytes: Option<u64>,
    pub write_bytes: Option<u64>,
    pub timestamp: u64,
    /// Key: PCI Slot ID of the GPU
    pub gpu_usage_stats: BTreeMap<PciSlot, GpuUsageStats>,
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

    fn get_uid(proc_path: &PathBuf) -> Result<u32> {
        let status = std::fs::read_to_string(proc_path.join("status"))?;
        if let Some(captures) = RE_UID.captures(&status) {
            let first_num_str = captures.get(1).context("no uid found")?;
            first_num_str
                .as_str()
                .parse::<u32>()
                .context("couldn't parse uid in /status")
        } else {
            Ok(0)
        }
    }

    pub fn update_nvidia_stats() {
        {
            let mut stats = NVIDIA_PROCESSES_STATS.write().unwrap();
            stats.clear();
            stats.extend(Self::nvidia_process_stats());
        }
        {
            let mut infos = NVIDIA_PROCESS_INFOS.write().unwrap();
            infos.clear();
            infos.extend(Self::nvidia_process_infos());
        }
    }

    pub fn all_process_data() -> Result<Vec<Self>> {
        Self::update_nvidia_stats();

        let mut process_data = vec![];
        for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
            let data = ProcessData::try_from_path(entry);

            if let Ok(data) = data {
                process_data.push(data)
            }
        }

        Ok(process_data)
    }

    pub fn try_from_path(proc_path: PathBuf) -> Result<Self> {
        let stat = std::fs::read_to_string(&proc_path.join("stat"))?;
        let statm = std::fs::read_to_string(&proc_path.join("statm"))?;
        let comm = std::fs::read_to_string(&proc_path.join("comm"))?;
        let commandline = std::fs::read_to_string(&proc_path.join("cmdline"))?;
        let cgroup = std::fs::read_to_string(&proc_path.join("cgroup"))?;
        let io = std::fs::read_to_string(&proc_path.join("io")).ok();

        let pid = proc_path
            .file_name()
            .context("proc_path terminates in ..")?
            .to_str()
            .context("can't turn OsStr to str")?
            .parse()?;

        let uid = Self::get_uid(&proc_path)?;

        let stat = stat
            .split(')') // since we don't care about the pid or the executable name, split after the executable name to make our life easier
            .last()
            .context("stat doesn't have ')'")?
            .split(' ')
            .skip(1) // the first element would be a space, let's ignore that
            .collect::<Vec<_>>();

        let statm = statm.split(' ').collect::<Vec<_>>();

        let comm = comm.replace('\n', "");

        // -2 to accommodate for only collecting after the second item (which is the executable name as mentioned above)
        let user_cpu_time = stat[13 - 2].parse::<u64>()?;
        let system_cpu_time = stat[14 - 2].parse::<u64>()?;

        let cpu_time_timestamp = unix_as_millis();

        let memory_usage = (statm[1].parse::<usize>()? - statm[2].parse::<usize>()?) * *PAGESIZE;

        let starttime = stat[21 - 2].parse()?;

        let cgroup = Self::sanitize_cgroup(cgroup);

        let containerization = match &proc_path.join("root").join(".flatpak-info").exists() {
            true => Containerization::Flatpak,
            false => match commandline.starts_with("/snap/") {
                true => Containerization::Snap,
                false => Containerization::None,
            },
        };

        let read_bytes = io.as_ref().and_then(|io| {
            RE_IO_READ
                .captures(io)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
        });

        let write_bytes = io.as_ref().and_then(|io| {
            RE_IO_WRITE
                .captures(io)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
        });

        let gpu_usage_stats = Self::gpu_usage_stats(&proc_path, pid);

        let timestamp = unix_as_millis();

        Ok(Self {
            pid,
            uid,
            comm,
            commandline,
            user_cpu_time,
            system_cpu_time,
            cpu_time_timestamp,
            memory_usage,
            starttime,
            cgroup,
            proc_path,
            containerization,
            read_bytes,
            write_bytes,
            timestamp,
            gpu_usage_stats,
        })
    }

    fn gpu_usage_stats(proc_path: &PathBuf, pid: i32) -> BTreeMap<PciSlot, GpuUsageStats> {
        let nvidia_stats = Self::nvidia_gpu_stats_all(pid).unwrap_or_default();
        let mut other_stats = Self::other_gpu_usage_stats(proc_path, pid).unwrap_or_default();
        other_stats.extend(nvidia_stats.into_iter());
        other_stats
    }

    fn other_gpu_usage_stats(
        proc_path: &PathBuf,
        pid: i32,
    ) -> Result<BTreeMap<PciSlot, GpuUsageStats>> {
        let fdinfo_dir = proc_path.join("fdinfo");

        let mut seen_fds = HashSet::new();

        let mut return_map = BTreeMap::new();
        for entry in std::fs::read_dir(fdinfo_dir)? {
            let entry = entry?;
            let fdinfo_path = entry.path();

            let _file = std::fs::File::open(&fdinfo_path);
            if _file.is_err() {
                continue;
            }
            let mut file = _file.unwrap();

            let _metadata = file.metadata();
            if _metadata.is_err() {
                continue;
            }
            let metadata = _metadata.unwrap();

            // if our fd is 0, 1 or 2 it's probably just a std stream so skip it
            let fd_num = fdinfo_path
                .file_name()
                .and_then(|osstr| osstr.to_str())
                .unwrap_or("0")
                .parse::<usize>()
                .unwrap_or(0);
            if fd_num <= 2 {
                continue;
            }

            if !metadata.is_file() {
                continue;
            }

            // Adapted from nvtop's `is_drm_fd()`
            // https://github.com/Syllo/nvtop/blob/master/src/extract_processinfo_fdinfo.c
            let fd_path = fdinfo_path.to_str().map(|s| s.replace("fdinfo", "fd"));
            if let Some(fd_path) = fd_path {
                if let Ok(fd_metadata) = std::fs::metadata(fd_path) {
                    let major = unsafe { libc::major(fd_metadata.st_rdev()) };
                    if (fd_metadata.st_mode() & libc::S_IFMT) != libc::S_IFCHR || major != 226 {
                        continue;
                    }
                }
            }

            // Adapted from nvtop's `processinfo_sweep_fdinfos()`
            // https://github.com/Syllo/nvtop/blob/master/src/extract_processinfo_fdinfo.c
            // if we've already seen the file this fd refers to, skip
            let not_unique = seen_fds.iter().any(|seen_fd| unsafe {
                syscalls::syscall!(syscalls::Sysno::kcmp, pid, pid, 0, fd_num, *seen_fd)
                    .unwrap_or(0)
                    == 0
            });
            if not_unique {
                continue;
            }

            seen_fds.insert(fd_num);

            if let Ok(stats) = Self::read_fdinfo(&mut file, metadata.len() as usize) {
                return_map
                    .entry(stats.0)
                    .and_modify(|existing_value: &mut GpuUsageStats| {
                        if stats.1.gfx > existing_value.gfx {
                            existing_value.gfx = stats.1.gfx;
                        }
                        if stats.1.dec > existing_value.dec {
                            existing_value.dec = stats.1.dec;
                        }
                        if stats.1.enc > existing_value.enc {
                            existing_value.enc = stats.1.enc;
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

    fn read_fdinfo(
        fdinfo_file: &mut File,
        file_size: usize,
    ) -> Result<(PciSlot, GpuUsageStats, i64)> {
        let mut content = String::with_capacity(file_size);
        fdinfo_file.read_to_string(&mut content)?;
        fdinfo_file.flush()?;

        let pci_slot = RE_DRM_PDEV
            .captures(&content)
            .and_then(|captures| captures.get(1))
            .and_then(|capture| PciSlot::from_str(capture.as_str()).ok());

        let client_id = RE_DRM_CLIENT_ID
            .captures(&content)
            .and_then(|captures| captures.get(1))
            .and_then(|capture| capture.as_str().parse::<i64>().ok());

        if let (Some(pci_slot), Some(client_id)) = (pci_slot, client_id) {
            let gfx = RE_DRM_ENGINE_GFX // amd
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .or_else(|| {
                    // intel
                    RE_DRM_ENGINE_RENDER
                        .captures(&content)
                        .and_then(|captures| captures.get(1))
                        .and_then(|capture| capture.as_str().parse::<u64>().ok())
                })
                .unwrap_or_default();

            let compute = RE_DRM_ENGINE_COMPUTE
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default();

            let enc = RE_DRM_ENGINE_ENC // amd
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .or_else(|| {
                    // intel
                    RE_DRM_ENGINE_VIDEO
                        .captures(&content)
                        .and_then(|captures| captures.get(1))
                        .and_then(|capture| capture.as_str().parse::<u64>().ok())
                })
                .unwrap_or_default();

            let dec = RE_DRM_ENGINE_DEC
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default();

            let vram = RE_DRM_MEMORY_VRAM
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default()
                * 1024;

            let gtt = RE_DRM_MEMORY_GTT
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
                .unwrap_or_default()
                * 1024;

            let stats = GpuUsageStats {
                gfx: gfx + compute,
                mem: vram.saturating_add(gtt),
                enc,
                dec,
                nvidia: false,
            };

            return Ok((pci_slot, stats, client_id));
        }

        bail!("unable to find gpu information in this fdinfo");
    }

    fn nvidia_gpu_stats_all(pid: i32) -> Result<BTreeMap<PciSlot, GpuUsageStats>> {
        let mut return_map = BTreeMap::new();

        for (pci_slot, _) in NVML_DEVICES.iter() {
            if let Ok(stats) = Self::nvidia_gpu_stats(pid, *pci_slot) {
                return_map.insert(pci_slot.to_owned(), stats);
            }
        }

        Ok(return_map)
    }

    fn nvidia_gpu_stats(pid: i32, pci_slot: PciSlot) -> Result<GpuUsageStats> {
        let this_process_stats = NVIDIA_PROCESSES_STATS
            .read()
            .unwrap()
            .get(&pci_slot)
            .context("couldn't find GPU with this PCI slot")?
            .iter()
            .filter(|process| process.pid == pid as u32)
            .map(|stats| (stats.sm_util, stats.enc_util, stats.dec_util))
            .reduce(|acc, curr| (acc.0 + curr.0, acc.1 + curr.1, acc.2 + curr.2));

        let this_process_mem_stats: u64 = NVIDIA_PROCESS_INFOS
            .read()
            .unwrap()
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
            mem: this_process_mem_stats,
            enc: this_process_stats.unwrap_or_default().1 as u64,
            dec: this_process_stats.unwrap_or_default().2 as u64,
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
                gpu.process_utilization_stats(unix_as_millis() * 1000 - 5_000_000)
                    .unwrap_or_default(),
            );
        }

        return_map
    }
}

pub fn unix_as_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
