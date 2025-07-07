pub mod gpu_usage;
pub mod pci_slot;

use anyhow::{Context, Result, bail};
use glob::glob;
use lazy_regex::{Lazy, Regex, lazy_regex};
use nutype::nutype;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::struct_wrappers::device::{ProcessInfo, ProcessUtilizationSample};
use nvml_wrapper::{Device, Nvml};
use pci_slot::PciSlot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::iter::Sum;
use std::os::linux::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{LazyLock, RwLock};
use std::time::SystemTime;

use crate::gpu_usage::{GpuIdentifier, GpuUsageStats, Percentage};

const STAT_OFFSET: usize = 2; // we split the stat contents where the executable name ends, which is the second element
const STAT_PARENT_PID: usize = 3 - STAT_OFFSET;
const STAT_USER_CPU_TIME: usize = 13 - STAT_OFFSET;
const STAT_SYSTEM_CPU_TIME: usize = 14 - STAT_OFFSET;
const STAT_NICE: usize = 18 - STAT_OFFSET;
const STAT_STARTTIME: usize = 21 - STAT_OFFSET;

const DRM_DRIVER: &str = "drm-driver";

const DRM_PDEV: &str = "drm-pdev";

static USERS_CACHE: LazyLock<HashMap<libc::uid_t, String>> = LazyLock::new(|| unsafe {
    uzers::all_users()
        .map(|user| (user.uid(), user.name().to_string_lossy().to_string()))
        .collect()
});

static PAGESIZE: LazyLock<usize> = LazyLock::new(sysconf::pagesize);

static NUM_CPUS: LazyLock<usize> = LazyLock::new(num_cpus::get);

static RE_UID: Lazy<Regex> = lazy_regex!(r"Uid:\s*(\d+)");

static RE_AFFINITY: Lazy<Regex> = lazy_regex!(r"Cpus_allowed:\s*([0-9A-Fa-f]+)");

static RE_SWAP_USAGE: Lazy<Regex> = lazy_regex!(r"VmSwap:\s*([0-9]+)\s*kB");

static RE_IO_READ: Lazy<Regex> = lazy_regex!(r"read_bytes:\s*(\d+)");

static RE_IO_WRITE: Lazy<Regex> = lazy_regex!(r"write_bytes:\s*(\d+)");

static RE_DRM_KIB: Lazy<Regex> = lazy_regex!(r"(\d+)\s*KiB");

static RE_DRM_TIME: Lazy<Regex> = lazy_regex!(r"(\d+)\s*ns");

static RE_DRM_UNITS: Lazy<Regex> = lazy_regex!(r"(\d+)");

static GFX_NS_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> = Lazy::new(|| {
    HashMap::from_iter([
        ("amdgpu", vec!["drm-engine-compute", "drm-engine-gfx"]),
        ("i915", vec!["drm-engine-render"]),
        ("v3d", vec!["drm-engine-render"]),
    ])
});

static GFX_CYCLES_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> =
    Lazy::new(|| HashMap::from_iter([("xe", vec!["drm-cycles-rcs", "drm-cycles-ccs"])]));

static GFX_TOTAL_CYCLES_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> = Lazy::new(|| {
    HashMap::from_iter([("xe", vec!["drm-total-cycles-rcs", "drm-total-cycles-ccs"])])
});

static ENC_NS_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> = Lazy::new(|| {
    HashMap::from_iter([
        ("amdgpu", vec!["drm-engine-enc"]),
        ("i915", vec!["drm-engine-video"]),
    ])
});

static ENC_CYCLES_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> =
    Lazy::new(|| HashMap::from_iter([("xe", vec!["drm-cycles-vcs"])]));

static ENC_TOTAL_CYCLES_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> =
    Lazy::new(|| HashMap::from_iter([("xe", vec!["drm-total-cycles-vcs"])]));

static DEC_NS_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> =
    Lazy::new(|| HashMap::from_iter([("amdgpu", vec!["drm-engine-dec"])]));

static MEM_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> = Lazy::new(|| {
    HashMap::from_iter([
        ("amdgpu", vec!["drm-memory-gtt", "drm-memory-vram"]),
        ("v3d", vec!["drm-total-memory"]),
        ("xe", vec!["drm-engine-vram0"]),
    ])
});

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
    Debug, Clone, Default, Hash, PartialEq, Eq, Serialize, Deserialize, Copy, PartialOrd, Ord,
)]
pub enum Containerization {
    #[default]
    None,
    Flatpak,
    Snap,
}

/// Data that could be transferred us>ing `resources-processes`, separated from
/// `Process` mainly due to `Icon` not being able to derive `Serialize` and
/// `Deserialize`.
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

    fn get_uid(proc_path: &Path) -> Result<u32> {
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
            let data = ProcessData::try_from_path(&entry);

            if let Ok(data) = data {
                process_data.push(data);
            }
        }

        Ok(process_data)
    }

    pub fn try_from_path<P: AsRef<Path>>(proc_path: P) -> Result<Self> {
        let proc_path = proc_path.as_ref();
        let stat = std::fs::read_to_string(proc_path.join("stat"))?;
        let statm = std::fs::read_to_string(proc_path.join("statm"))?;
        let status = std::fs::read_to_string(proc_path.join("status"))?;
        let comm = std::fs::read_to_string(proc_path.join("comm"))?;
        let commandline = std::fs::read_to_string(proc_path.join("cmdline"))?;
        let io = std::fs::read_to_string(proc_path.join("io")).ok();

        let pid = proc_path
            .file_name()
            .context("proc_path terminates in ..")?
            .to_str()
            .context("can't turn OsStr to str")?
            .parse()?;

        let user = USERS_CACHE
            .get(&Self::get_uid(proc_path)?)
            .cloned()
            .unwrap_or(String::from("root"));

        let stat = stat
            .split(')') // since we don't care about the pid or the executable name, split after the executable name to make our life easier
            .last()
            .context("stat doesn't have ')'")?
            .split(' ')
            .skip(1) // the first element would be a space, let's ignore that
            .collect::<Vec<_>>();

        let statm = statm.split(' ').collect::<Vec<_>>();

        let comm = comm.replace('\n', "");

        let parent_pid = stat
            .get(STAT_PARENT_PID)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content"))?;
        let user_cpu_time = stat
            .get(STAT_USER_CPU_TIME)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content"))?;
        let system_cpu_time = stat
            .get(STAT_SYSTEM_CPU_TIME)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content"))?;
        let nice = stat
            .get(STAT_NICE)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content"))?;
        let starttime = stat
            .get(STAT_STARTTIME)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content"))?;

        let mut affinity = Vec::with_capacity(*NUM_CPUS);
        RE_AFFINITY
            .captures(&status)
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str())
            .unwrap_or_default()
            .chars()
            .map(|c| c.to_digit(16).unwrap_or_default())
            .rev()
            .for_each(|int| {
                // we want the bits and there are 4 bits in a hex digit
                (0..4).for_each(|i| {
                    // this if should prevent wrong size affinity vecs if the thread count is not divisible by 4
                    if affinity.len() < *NUM_CPUS {
                        affinity.push((int & (1 << i)) != 0);
                    }
                });
            });

        let swap_usage = RE_SWAP_USAGE
            .captures(&status)
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str())
            .unwrap_or_default()
            .parse::<usize>()
            .unwrap_or_default()
            .saturating_mul(1000);

        let memory_usage = statm
            .get(1)
            .context("wrong statm file format")
            .and_then(|x| {
                x.parse::<usize>()
                    .context("couldn't parse statm file content")
            })?
            .saturating_sub(
                statm
                    .get(2)
                    .context("wrong statm file format")
                    .and_then(|x| {
                        x.parse::<usize>()
                            .context("couldn't parse statm file content")
                    })?,
            )
            .saturating_mul(*PAGESIZE);

        let cgroup = std::fs::read_to_string(proc_path.join("cgroup"))
            .ok()
            .and_then(Self::sanitize_cgroup);

        let containerization = if commandline.starts_with("/snap/") {
            Containerization::Snap
        } else if proc_path.join("root").join(".flatpak-info").exists() {
            Containerization::Flatpak
        } else {
            Containerization::None
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

        let fdinfos = Self::collect_fdinfos(pid).unwrap_or_default();

        let nvidia_stats = Self::nvidia_gpu_stats_all(pid);
        let mut gpu_usage_stats = Self::other_gpu_usage_stats(&fdinfos).unwrap_or_default();
        gpu_usage_stats.extend(nvidia_stats);

        let timestamp = unix_as_millis();

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
        })
    }

    fn collect_fdinfos(pid: libc::pid_t) -> Result<Vec<HashMap<String, String>>> {
        let fdinfo_dir = PathBuf::from(format!("/proc/{pid}/fdinfo"));

        let mut seen_fds = HashSet::new();

        let mut return_vec = Vec::new();

        for entry in std::fs::read_dir(fdinfo_dir)? {
            let entry = entry?;
            let fdinfo_path = entry.path();

            let _file = std::fs::File::open(&fdinfo_path);
            if _file.is_err() {
                continue;
            }
            let mut file = _file.unwrap();

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

            let _metadata = file.metadata();
            if _metadata.is_err() {
                continue;
            }
            let metadata = _metadata.unwrap();

            if !metadata.is_file() {
                continue;
            }

            // Adapted from nvtop's `is_drm_fd()`
            // https://github.com/Syllo/nvtop/blob/master/src/extract_processinfo_fdinfo.c
            let fd_path = fdinfo_path.to_str().map(|s| s.replace("fdinfo", "fd"));
            if let Some(fd_path) = fd_path {
                if let Ok(fd_metadata) = std::fs::metadata(fd_path) {
                    let major = libc::major(fd_metadata.st_rdev());
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

            let mut buffer = String::new();
            file.read_to_string(&mut buffer)?;

            return_vec.push(Self::parse_fdinfo(buffer));
        }

        Ok(return_vec)
    }

    fn parse_fdinfo<S: Into<String>>(file_contents: S) -> HashMap<String, String> {
        HashMap::from_iter(
            file_contents
                .into()
                .lines()
                .filter_map(|line| line.split_once(':'))
                .map(|(name, value)| (name.trim().to_string(), value.trim().to_string())),
        )
    }

    fn other_gpu_usage_stats(
        fdinfos: &[HashMap<String, String>],
    ) -> Result<BTreeMap<GpuIdentifier, GpuUsageStats>> {
        let mut return_map = BTreeMap::new();

        for fdinfo in fdinfos {
            if let Ok((identifier, stats)) = Self::extract_gpu_usage_from_fdinfo(fdinfo) {
                return_map
                    .entry(identifier)
                    .and_modify(|existing_value: &mut GpuUsageStats| {
                        *existing_value = existing_value.greater(&stats)
                    })
                    .or_insert(stats);
            }
        }

        Ok(return_map)
    }

    fn extract_gpu_usage_from_fdinfo(
        fdinfo: &HashMap<String, String>,
    ) -> Result<(GpuIdentifier, GpuUsageStats)> {
        let driver = fdinfo.get(DRM_DRIVER);

        if let Some(driver) = driver {
            let gpu_identifier = fdinfo
                .get(DRM_PDEV)
                .and_then(|field| PciSlot::from_str(field).ok())
                .map(GpuIdentifier::PciSlot)
                .unwrap_or_default();

            let stats = match driver.as_str() {
                // TODO: this surely can be made prettier
                "amdgpu" => GpuUsageStats::AmdgpuStats {
                    gfx_ns: GFX_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    enc_ns: ENC_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    dec_ns: DEC_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    mem_bytes: MEM_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| {
                            Self::parse_drm_fields::<u64, _>(fdinfo, names, &RE_DRM_KIB)
                                .saturating_mul(1024)
                        })
                        .unwrap_or_default(),
                },
                "i915" => GpuUsageStats::I915Stats {
                    gfx_ns: GFX_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    video_ns: ENC_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                },
                "v3d" => GpuUsageStats::V3dStats {
                    gfx_ns: GFX_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    mem_bytes: MEM_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| {
                            Self::parse_drm_fields::<u64, _>(fdinfo, names, &RE_DRM_KIB)
                                .saturating_mul(1024)
                        })
                        .unwrap_or_default(),
                },
                "xe" => GpuUsageStats::XeStats {
                    gfx_cycles: GFX_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    gfx_total_cycles: GFX_TOTAL_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    video_cycles: ENC_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    video_total_cycles: ENC_TOTAL_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    mem_bytes: MEM_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| {
                            Self::parse_drm_fields::<u64, _>(fdinfo, names, &RE_DRM_KIB)
                                .saturating_mul(1024)
                        })
                        .unwrap_or_default(),
                },
                _ => bail!("unable to read stats from driver"),
            };

            return Ok((gpu_identifier, stats));
        }

        bail!("unable to find gpu information in this fdinfo");
    }

    fn parse_drm_fields<T: FromStr + Sum, S: AsRef<str>>(
        fdinfo: &HashMap<String, String>,
        field_names: &[S],
        regex: &Regex,
    ) -> T {
        field_names
            .iter()
            .filter_map(|name| {
                fdinfo.get(name.as_ref()).and_then(|value| {
                    regex
                        .captures(&value)
                        .and_then(|captures| captures.get(1))
                        .and_then(|capture| capture.as_str().parse::<T>().ok())
                })
            })
            .sum()
    }

    fn nvidia_gpu_stats_all(pid: i32) -> BTreeMap<GpuIdentifier, GpuUsageStats> {
        let mut return_map = BTreeMap::new();

        for (pci_slot, _) in NVML_DEVICES.iter() {
            if let Ok(stats) = Self::nvidia_gpu_stats(pid, *pci_slot) {
                return_map.insert(GpuIdentifier::PciSlot(pci_slot.to_owned()), stats);
            }
        }

        return_map
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

        let gpu_stats = GpuUsageStats::NvidiaStats {
            gfx_percentage: Percentage::try_new(this_process_stats.unwrap_or_default().0 as u8)?,
            mem_bytes: this_process_mem_stats,
            enc_percentage: Percentage::try_new(this_process_stats.unwrap_or_default().1 as u8)?,
            dec_percentage: Percentage::try_new(this_process_stats.unwrap_or_default().2 as u8)?,
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
                gpu.process_utilization_stats(
                    unix_as_millis()
                        .saturating_mul(1000)
                        .saturating_sub(5_000_000),
                )
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
