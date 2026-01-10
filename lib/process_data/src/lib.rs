pub mod gpu_usage;
pub mod npu_usage;
pub mod pci_slot;

use anyhow::{Context, Result, bail};
use lazy_regex::{Lazy, Regex, lazy_regex};
use log::{debug, trace, warn};
use nutype::nutype;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::struct_wrappers::device::{ProcessInfo, ProcessUtilizationSample};
use nvml_wrapper::{Device, Nvml};
use pci_slot::PciSlot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::iter::Sum;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{LazyLock, RwLock};
use std::time::SystemTime;

use crate::gpu_usage::{GpuIdentifier, GpuUsageStats, IntegerPercentage};
use crate::npu_usage::NpuUsageStats;

const STAT_OFFSET: usize = 2; // we split the stat contents where the executable name ends, which is the second element
const STAT_PARENT_PID: usize = 3 - STAT_OFFSET;
const STAT_USER_CPU_TIME: usize = 13 - STAT_OFFSET;
const STAT_SYSTEM_CPU_TIME: usize = 14 - STAT_OFFSET;
const STAT_NICE: usize = 18 - STAT_OFFSET;
const STAT_STARTTIME: usize = 21 - STAT_OFFSET;

const DRM_DRIVER: &str = "drm-driver";

const DRM_PDEV: &str = "drm-pdev";

static USERS_CACHE: LazyLock<HashMap<libc::uid_t, String>> = LazyLock::new(|| unsafe {
    debug!("Initializing users cache…");
    let users: HashMap<libc::uid_t, String> = uzers::all_users()
        .map(|user| {
            trace!("Found user {}", user.name().to_string_lossy());
            (user.uid(), user.name().to_string_lossy().to_string())
        })
        .collect();
    debug!("Found {} users", users.len());
    users
});

static NUM_CPUS: LazyLock<usize> = LazyLock::new(num_cpus::get);

static RE_UID: Lazy<Regex> = lazy_regex!(r"Uid:\s*(\d+)");

static RE_CGROUP: Lazy<Regex> = lazy_regex!(
    r"(?U)/(?:app|background)\.slice/(?:app-|dbus-:)(?:(?P<launcher>[^-]+)-)?(?P<cgroup>[^-]+)(?:-[0-9]+|@[0-9]+)?\.(?:scope|service)"
);

static RE_AFFINITY: Lazy<Regex> = lazy_regex!(r"Cpus_allowed:\s*([0-9A-Fa-f]+)");

static RE_MEMORY_USAGE: Lazy<Regex> = lazy_regex!(r"VmRSS:\s*([0-9]+)\s*kB");

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
    Lazy::new(|| HashMap::from_iter([("xe", vec!["drm-cycles-rcs"])]));

static GFX_TOTAL_CYCLES_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> =
    Lazy::new(|| HashMap::from_iter([("xe", vec!["drm-total-cycles-rcs"])]));

static COMPUTE_CYCLES_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> =
    Lazy::new(|| HashMap::from_iter([("xe", vec!["drm-cycles-ccs"])]));

static COMPUTE_TOTAL_CYCLES_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> =
    Lazy::new(|| HashMap::from_iter([("xe", vec!["drm-total-cycles-ccs"])]));

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

static NPU_NS_FIELDS: Lazy<HashMap<&str, Vec<&str>>> =
    Lazy::new(|| HashMap::from_iter([("amdxdna_accel_driver", vec!["drm-engine-npu-amdxdna"])]));

static MEM_DRM_FIELDS: Lazy<HashMap<&str, Vec<&str>>> = Lazy::new(|| {
    HashMap::from_iter([
        ("amdgpu", vec!["drm-memory-gtt", "drm-memory-vram"]),
        ("amdxdna_accel_driver", vec!["drm-total-memory"]),
        ("i915", vec!["drm-total-local0", "drm-total-system0"]),
        ("v3d", vec!["drm-total-memory"]),
        ("xe", vec!["drm-total-gtt", "drm-total-vram0"]),
    ])
});

static NVML: Lazy<Result<Nvml, NvmlError>> = Lazy::new(|| {
    debug!("Initializing connection to NVML…");
    Nvml::init().inspect_err(|err| warn!("Unable to connect to NVML: {err}"))
});

static NVML_DEVICES: Lazy<Vec<(PciSlot, Device)>> = Lazy::new(|| {
    if let Ok(nvml) = NVML.as_ref() {
        debug!("Looking for NVIDIA devices…");
        let device_count = nvml.device_count().unwrap_or(0);
        let mut return_vec = Vec::with_capacity(device_count as usize);
        for i in 0..device_count {
            if let Ok(gpu) = nvml.device_by_index(i) {
                if let Ok(pci_slot) = gpu.pci_info().map(|pci_info| pci_info.bus_id) {
                    let pci_slot = PciSlot::from_str(&pci_slot).unwrap();
                    debug!(
                        "Found {} at {}",
                        gpu.name().unwrap_or("N/A".into()),
                        pci_slot
                    );
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
    Portable,
    Snap,
    AppImage,
}

#[derive(Debug, Clone)]
struct Fdinfo {
    pub pid: libc::pid_t,
    pub fdinfo_num: usize,
    pub content: HashMap<String, String>,
}

/// Data that could be transferred using `resources-processes`, separated from
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
    pub npu_usage_stats: BTreeMap<PciSlot, NpuUsageStats>,
    pub appimage_path: Option<String>,
}

impl ProcessData {
    // apparently some apps like Mullvad do this to include a '-' in their cgroup even though that's not allowed
    fn decode_hex_escapes(s: &str) -> Result<String, ()> {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;

        #[inline]
        const fn hex(b: u8) -> Result<u8, ()> {
            match b {
                b'0'..=b'9' => Ok(b - b'0'),
                b'a'..=b'f' => Ok(b - b'a' + 10),
                b'A'..=b'F' => Ok(b - b'A' + 10),
                _ => Err(()),
            }
        }

        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1] == b'x' {
                let hi = bytes[i + 2];
                let lo = bytes[i + 3];

                let val = (hex(hi)? << 4) | hex(lo)?;
                out.push(val);
                i += 4;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }

        String::from_utf8(out).map_err(|_| ())
    }

    fn sanitize_cgroup<S: AsRef<str>>(cgroup: S) -> (Option<String>, Option<String>) {
        RE_CGROUP
            .captures(cgroup.as_ref())
            .map(|captures| {
                (
                    captures
                        .name("launcher")
                        .and_then(|s| Self::decode_hex_escapes(s.as_str()).ok()),
                    captures
                        .name("cgroup")
                        .and_then(|s| Self::decode_hex_escapes(s.as_str()).ok()),
                )
            })
            .unwrap_or_default()
    }

    fn get_uid(status: &str) -> Result<u32> {
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

    pub fn all_process_data(
        non_gpu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
        non_npu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
        symlink_cache: &mut HashMap<(libc::pid_t, usize), PathBuf>,
    ) -> Result<Vec<Self>> {
        Self::update_nvidia_stats();

        let mut process_data = vec![];
        for entry in std::fs::read_dir("/proc")?.flatten() {
            // if name contains pid
            if let Ok(_) = entry.file_name().to_string_lossy().parse::<u32>() {
                let data = ProcessData::try_from_path(
                    entry.path(),
                    non_gpu_fdinfos,
                    non_npu_fdinfos,
                    symlink_cache,
                );

                if let Ok(data) = data {
                    process_data.push(data);
                }
            }
        }

        Ok(process_data)
    }

    pub fn try_from_path<P: AsRef<Path>>(
        proc_path: P,
        non_gpu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
        non_npu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
        symlink_cache: &mut HashMap<(libc::pid_t, usize), PathBuf>,
    ) -> Result<Self> {
        let proc_path = proc_path.as_ref();
        let pid = proc_path
            .file_name()
            .context("proc_path terminates in ..")?
            .to_str()
            .context("can't turn OsStr to str")?
            .parse()?;

        trace!("Inspecting process {pid}…");

        let stat = read_parsed::<String>(proc_path.join("stat"))?;

        let status = read_parsed::<String>(proc_path.join("status"))?;

        let comm = read_parsed::<String>(proc_path.join("comm"))?;

        let commandline = read_parsed::<String>(proc_path.join("cmdline"))?;

        let io = read_parsed::<String>(proc_path.join("io")).ok();

        let user = USERS_CACHE
            .get(&Self::get_uid(&status)?)
            .cloned()
            .unwrap_or(String::from("root"));
        trace!("User of {pid} determined to be {user}");

        let stat = stat
            .split(')') // since we don't care about the pid or the executable name, split after the executable name to make our life easier
            .last()
            .context("stat doesn't have ')'")
            .inspect_err(|err| trace!("Can't parse 'stat': {err}"))?
            .split(' ')
            .skip(1) // the first element would be a space, let's ignore that
            .collect::<Vec<_>>();

        let comm = comm.replace('\n', "");
        trace!("Comm of {pid} determined to be {comm}");

        let parent_pid = stat
            .get(STAT_PARENT_PID)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content to int"))
            .inspect_err(|err| trace!("Can't parse parent pid from 'stat': {err}"))?;
        trace!("Parent pid of {pid} determined to be {parent_pid}");

        let user_cpu_time = stat
            .get(STAT_USER_CPU_TIME)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content to int"))
            .inspect_err(|err| trace!("Can't parse user cpu time from 'stat': {err}"))?;
        trace!("User CPU time of {pid} determined to be {user_cpu_time}");

        let system_cpu_time = stat
            .get(STAT_SYSTEM_CPU_TIME)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content to int"))
            .inspect_err(|err| trace!("Can't parse system cpu time from 'stat': {err}"))?;
        trace!("System CPU time of {pid} determined to be {system_cpu_time}");

        let nice = stat
            .get(STAT_NICE)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content to int"))
            .inspect_err(|err| trace!("Can't parse nice from 'stat': {err}"))?;
        trace!("Nice of {pid} determined to be {nice}");

        let starttime = stat
            .get(STAT_STARTTIME)
            .context("wrong stat file format")
            .and_then(|x| x.parse().context("couldn't parse stat file content to int"))
            .inspect_err(|err| trace!("Can't parse start time from 'stat': {err}"))?;
        trace!("Starttime of {pid} determined to be {starttime}");

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
                    // this should prevent wrong size affinity vecs if the thread count is not divisible by 4
                    if affinity.len() < *NUM_CPUS {
                        affinity.push((int & (1 << i)) != 0);
                    }
                });
            });
        trace!("Affinity of {pid} determined to be {affinity:?}");

        let swap_usage = RE_SWAP_USAGE
            .captures(&status)
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str())
            .unwrap_or_default()
            .parse::<usize>()
            .unwrap_or_default()
            .saturating_mul(1000);
        trace!("Swap usage of {pid} determined to be {swap_usage}");

        let memory_usage = RE_MEMORY_USAGE
            .captures(&status)
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str())
            .unwrap_or_default()
            .parse::<usize>()
            .unwrap_or_default()
            .saturating_mul(1000);
        trace!("Memory usage of {pid} determined to be {swap_usage}");

        let (launcher, cgroup) = read_parsed::<String>(proc_path.join("cgroup"))
            .map(Self::sanitize_cgroup)
            .map(|(launcher, cgroup)| (launcher.unwrap_or_default(), cgroup)) // we only need the launcher for some checks here locally
            .unwrap_or_default();
        trace!("Launcher of {pid} determined to be {launcher}");
        trace!("Cgroup of {pid} determined to be {cgroup:?}");

        let environ = read_parsed::<String>(proc_path.join("environ"))?
            .split('\0')
            .filter_map(|e| e.split_once('='))
            .map(|(x, y)| (x.to_string(), y.to_string()))
            .collect::<HashMap<_, _>>();

        let appimage_path = environ.get("APPIMAGE").map(|p| p.to_owned());

        let containerization = if commandline.starts_with("/snap/") {
            Containerization::Snap
        } else if proc_path
            .join("root")
            .join("top.kimiblock.portable")
            .exists()
            || launcher == "portable"
        {
            Containerization::Portable
        } else if proc_path.join("root").join(".flatpak-info").exists() || launcher == "flatpak" {
            Containerization::Flatpak
        } else if appimage_path.is_some() {
            Containerization::AppImage
        } else {
            Containerization::None
        };

        trace!("Containerization of {pid} determined to be {containerization:?}");

        let read_bytes = io.as_ref().and_then(|io| {
            RE_IO_READ
                .captures(io)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
        });
        trace!("Read bytes of {pid} determined to be {read_bytes:?}");

        let write_bytes = io.as_ref().and_then(|io| {
            RE_IO_WRITE
                .captures(io)
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse::<u64>().ok())
        });
        trace!("Written bytes of {pid} determined to be {write_bytes:?}");

        trace!("Collecting fdinfo statistics for {pid}…");
        let fdinfos = Self::collect_fdinfos(pid, non_gpu_fdinfos, non_npu_fdinfos, symlink_cache)
            .unwrap_or_default();

        trace!("Collecting NVIDIA statistics for {pid}…");
        let nvidia_stats = Self::nvidia_gpu_stats_all(pid);
        let mut gpu_usage_stats =
            Self::other_gpu_usage_stats(&fdinfos, non_gpu_fdinfos).unwrap_or_default();
        gpu_usage_stats.extend(nvidia_stats);

        let npu_usage_stats = Self::npu_usage_stats(&fdinfos, non_npu_fdinfos).unwrap_or_default();

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

    fn collect_fdinfos(
        pid: libc::pid_t,
        non_gpu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
        non_npu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
        symlink_cache: &mut HashMap<(libc::pid_t, usize), PathBuf>,
    ) -> Result<Vec<Fdinfo>> {
        let fdinfo_dir = PathBuf::from(format!("/proc/{pid}/fdinfo"));
        let fd_dir = PathBuf::from(format!("/proc/{pid}/fd"));

        let mut seen_targets: HashMap<PathBuf, usize> = HashMap::new();
        let mut return_vec = Vec::new();

        for entry in std::fs::read_dir(fdinfo_dir)? {
            let entry = entry?;
            let fdinfo_path = entry.path();

            let fd_num = fdinfo_path
                .file_name()
                .and_then(|osstr| osstr.to_str())
                .unwrap_or("0")
                .parse::<usize>()
                .unwrap_or(0);

            let is_cached_non_gpu = non_gpu_fdinfos.contains(&(pid, fd_num));
            let is_cached_non_npu = non_npu_fdinfos.contains(&(pid, fd_num));

            let fd_symlink = fd_dir.join(fd_num.to_string());
            let symlink_target = {
                if let Some(cached_target) = symlink_cache.get(&(pid, fd_num)) {
                    Some(cached_target.clone())
                } else {
                    std::fs::read_link(&fd_symlink).ok()
                }
            };

            if is_cached_non_gpu && is_cached_non_npu {
                if let Some(ref target) = symlink_target {
                    // skip if target doesn't look like a GPU/NPU device
                    let target_str = target.to_string_lossy();
                    if !target_str.contains("/dev/dri/") && !target_str.contains("/dev/accel/") {
                        trace!(
                            "fdinfo is known to be not related with NPUs and GPUs (target: {:?}), skipping",
                            target
                        );
                        continue;
                    } else {
                        // target changed to a GPU/NPU device, remove from cache
                        trace!("fdinfo target changed to GPU/NPU device, removing from cache");
                        non_gpu_fdinfos.remove(&(pid, fd_num));
                        non_npu_fdinfos.remove(&(pid, fd_num));
                    }
                }
            }

            if let Some(ref target) = symlink_target {
                if let Some(&first_fd) = seen_targets.get(target) {
                    trace!(
                        "fdinfo {} points to same target as fd {} (target: {:?}), skipping",
                        fd_num, first_fd, target
                    );
                    continue;
                }
                seen_targets.insert(target.clone(), fd_num);
                symlink_cache.insert((pid, fd_num), target.clone());
            }

            trace!("fdinfo passed all checks, reading and parsing…");

            let _content = read_parsed::<String>(fdinfo_path);
            if _content.is_err() {
                trace!("couldn't read fdinfo, skipping…");
                non_gpu_fdinfos.insert((pid, fd_num));
                non_npu_fdinfos.insert((pid, fd_num));
                continue;
            }
            let content = _content.unwrap();

            return_vec.push(Fdinfo {
                pid,
                fdinfo_num: fd_num,
                content: Self::parse_fdinfo(content),
            });
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

    fn npu_usage_stats(
        fdinfos: &[Fdinfo],
        non_npu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
    ) -> Result<BTreeMap<PciSlot, NpuUsageStats>> {
        let mut return_map = BTreeMap::new();

        for fdinfo in fdinfos {
            if let Ok((identifier, stats)) = Self::extract_npu_usage_from_fdinfo(fdinfo) {
                trace!(
                    "Successfully got NPU statistics from fdinfo {}/{}",
                    fdinfo.pid, fdinfo.fdinfo_num
                );
                return_map
                    .entry(identifier)
                    .and_modify(|existing_value: &mut NpuUsageStats| {
                        *existing_value = existing_value.greater(&stats)
                    })
                    .or_insert(stats);
            } else {
                trace!(
                    "fdinfo {}/{} is not NPU-related, will be skipped in the future",
                    fdinfo.pid, fdinfo.fdinfo_num
                );
                non_npu_fdinfos.insert((fdinfo.pid, fdinfo.fdinfo_num));
            }
        }

        Ok(return_map)
    }

    fn extract_npu_usage_from_fdinfo(fdinfo: &Fdinfo) -> Result<(PciSlot, NpuUsageStats)> {
        let driver = fdinfo.content.get(DRM_DRIVER);

        if let Some(driver) = driver {
            let gpu_identifier = fdinfo
                .content
                .get(DRM_PDEV)
                .and_then(|field| PciSlot::from_str(field).ok())
                .unwrap_or_default();

            let stats = match driver.as_str() {
                "amdxdna_accel_driver" => NpuUsageStats::AmdxdnaStats {
                    usage_ns: NPU_NS_FIELDS
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
                _ => bail!("unable to read stats from driver"),
            };

            return Ok((gpu_identifier, stats));
        }

        bail!("unable to find gpu information in this fdinfo");
    }

    fn other_gpu_usage_stats(
        fdinfos: &[Fdinfo],
        non_gpu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
    ) -> Result<BTreeMap<GpuIdentifier, GpuUsageStats>> {
        let mut return_map = BTreeMap::new();

        for fdinfo in fdinfos {
            if let Ok((identifier, stats)) = Self::extract_gpu_usage_from_fdinfo(fdinfo) {
                trace!(
                    "Successfully got GPU statistics from fdinfo {}/{}",
                    fdinfo.pid, fdinfo.fdinfo_num
                );
                return_map
                    .entry(identifier)
                    .and_modify(|existing_value: &mut GpuUsageStats| {
                        *existing_value = existing_value.greater(&stats)
                    })
                    .or_insert(stats);
            } else {
                trace!(
                    "fdinfo {}/{} is not GPU-related, will be skipped in the future",
                    fdinfo.pid, fdinfo.fdinfo_num
                );
                non_gpu_fdinfos.insert((fdinfo.pid, fdinfo.fdinfo_num));
            }
        }

        Ok(return_map)
    }

    fn extract_gpu_usage_from_fdinfo(fdinfo: &Fdinfo) -> Result<(GpuIdentifier, GpuUsageStats)> {
        let driver = fdinfo.content.get(DRM_DRIVER);

        if let Some(driver) = driver {
            let gpu_identifier = fdinfo
                .content
                .get(DRM_PDEV)
                .and_then(|field| PciSlot::from_str(field).ok())
                .map(GpuIdentifier::PciSlot)
                .unwrap_or_default();

            let stats = match driver.as_str() {
                // TODO: this surely can be made prettier
                "amdgpu" => GpuUsageStats::AmdgpuStats {
                    gfx_ns: GFX_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    enc_ns: ENC_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    dec_ns: DEC_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    mem_bytes: MEM_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| {
                            Self::parse_drm_fields::<u64, _>(&fdinfo, names, &RE_DRM_KIB)
                                .saturating_mul(1024)
                        })
                        .unwrap_or_default(),
                },
                "i915" => GpuUsageStats::I915Stats {
                    gfx_ns: GFX_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    video_ns: ENC_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                },
                "v3d" => GpuUsageStats::V3dStats {
                    gfx_ns: GFX_NS_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_TIME))
                        .unwrap_or_default(),
                    mem_bytes: MEM_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| {
                            Self::parse_drm_fields::<u64, _>(&fdinfo, names, &RE_DRM_KIB)
                                .saturating_mul(1024)
                        })
                        .unwrap_or_default(),
                },
                "xe" => GpuUsageStats::XeStats {
                    gfx_cycles: GFX_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    gfx_total_cycles: GFX_TOTAL_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    compute_cycles: COMPUTE_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    compute_total_cycles: COMPUTE_TOTAL_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    video_cycles: ENC_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    video_total_cycles: ENC_TOTAL_CYCLES_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| Self::parse_drm_fields(&fdinfo, names, &RE_DRM_UNITS))
                        .unwrap_or_default(),
                    mem_bytes: MEM_DRM_FIELDS
                        .get(driver.as_str())
                        .map(|names| {
                            Self::parse_drm_fields::<u64, _>(&fdinfo, names, &RE_DRM_KIB)
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
        fdinfo: &Fdinfo,
        field_names: &[S],
        regex: &Regex,
    ) -> T {
        field_names
            .iter()
            .filter_map(|name| {
                fdinfo.content.get(name.as_ref()).and_then(|value| {
                    regex
                        .captures(&value)
                        .and_then(|captures| captures.get(1))
                        .and_then(|capture| capture.as_str().parse::<T>().ok())
                })
            })
            .sum()
    }

    fn nvidia_gpu_stats_all(pid: i32) -> BTreeMap<GpuIdentifier, GpuUsageStats> {
        trace!("Gathering NVIDIA GPU stats…");

        let mut return_map = BTreeMap::new();

        for (pci_slot, _) in NVML_DEVICES.iter() {
            if let Ok(stats) = Self::nvidia_gpu_stats(pid, *pci_slot) {
                return_map.insert(GpuIdentifier::PciSlot(pci_slot.to_owned()), stats);
            }
        }

        return_map
    }

    fn nvidia_gpu_stats(pid: i32, pci_slot: PciSlot) -> Result<GpuUsageStats> {
        trace!("Gathering GPU stats for NVIDIA GPU at {pci_slot}…");
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
            gfx_percentage: IntegerPercentage::try_new(
                this_process_stats.unwrap_or_default().0 as u8,
            )?,
            mem_bytes: this_process_mem_stats,
            enc_percentage: IntegerPercentage::try_new(
                this_process_stats.unwrap_or_default().1 as u8,
            )?,
            dec_percentage: IntegerPercentage::try_new(
                this_process_stats.unwrap_or_default().2 as u8,
            )?,
        };
        Ok(gpu_stats)
    }

    fn nvidia_process_infos() -> HashMap<PciSlot, Vec<ProcessInfo>> {
        trace!("Refreshing NVIDIA process infos…");
        let mut return_map = HashMap::new();

        for (pci_slot, gpu) in NVML_DEVICES.iter() {
            let mut comp_gfx_stats = gpu.running_graphics_processes().unwrap_or_default();
            comp_gfx_stats.extend(gpu.running_compute_processes().unwrap_or_default());

            return_map.insert(pci_slot.to_owned(), comp_gfx_stats);
        }

        return_map
    }

    fn nvidia_process_stats() -> HashMap<PciSlot, Vec<ProcessUtilizationSample>> {
        trace!("Refreshing NVIDIA process stats…");
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

pub fn unix_as_secs_f64() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

pub fn read_parsed<T: FromStr>(path: impl AsRef<Path>) -> Result<T>
where
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    let path = path.as_ref();

    let content = std::fs::read_to_string(path)
        .map(|content| content.trim().to_string())
        .inspect_err(|e| trace!("Unable to read {path:?} → {e}"))?;

    let type_name = std::any::type_name::<T>();

    content
        .parse::<T>()
        .inspect(|_| trace!("Successfully read {path:?} to {type_name} → {content}",))
        .inspect_err(|e| {
            trace!("Unable to parse {path:?} to {type_name} → {e}");
        })
        .with_context(|| format!("error parsing file {}", path.display()))
}
