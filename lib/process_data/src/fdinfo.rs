use anyhow::{Result, bail};
use lazy_regex::{Lazy, Regex, lazy_regex};
use log::trace;
use std::collections::HashMap;
use std::iter::Sum;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::cache::ProcessDataCache;
use crate::gpu_usage::{GpuIdentifier, GpuUsageStats};
use crate::npu_usage::NpuUsageStats;
use crate::pci_slot::PciSlot;
use crate::read_parsed;

static RE_KIB: Lazy<Regex> = lazy_regex!(r"(\d+)\s*KiB");
static RE_NS: Lazy<Regex> = lazy_regex!(r"(\d+)\s*ns");
static RE_UNITS: Lazy<Regex> = lazy_regex!(r"(\d+)");

const DRM_DRIVER: &str = "drm-driver";
const DRM_PDEV: &str = "drm-pdev";

/// Parsed contents of one `/proc/<pid>/fdinfo/<fd>` file
#[derive(Debug, Clone)]
pub struct Fdinfo {
    pub pid: libc::pid_t,
    pub fd_num: usize,
    pub fields: HashMap<String, String>,
}

impl Fdinfo {
    fn parse(pid: libc::pid_t, fd_num: usize, content: &str) -> Self {
        let fields = content
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .collect();
        Self {
            pid,
            fd_num,
            fields,
        }
    }

    fn sum_ns(&self, keys: &[&str]) -> u64 {
        self.sum_fields(keys, &RE_NS)
    }

    fn sum_kib_as_bytes(&self, keys: &[&str]) -> u64 {
        self.sum_fields::<u64>(keys, &RE_KIB).saturating_mul(1024)
    }

    fn sum_units(&self, keys: &[&str]) -> u64 {
        self.sum_fields(keys, &RE_UNITS)
    }

    fn sum_fields<T>(&self, keys: &[&str], regex: &Regex) -> T
    where
        T: FromStr + Sum + Default,
    {
        keys.iter()
            .filter_map(|&key| {
                let value = self.fields.get(key)?;
                let cap = regex.captures(value)?.get(1)?;
                cap.as_str().parse::<T>().ok()
            })
            .sum()
    }
}

pub fn collect_fdinfos(
    pid: libc::pid_t,
    proc_root: &Path,
    cache: &mut ProcessDataCache,
) -> Result<Vec<Fdinfo>> {
    let fdinfo_dir = proc_root.join(pid.to_string()).join("fdinfo");
    let fd_dir = proc_root.join(pid.to_string()).join("fd");

    // track targets we've already seen so we don't double-count aliased fds
    let mut seen_targets: HashMap<PathBuf, usize> = HashMap::new();
    let mut out = Vec::new();

    for entry in std::fs::read_dir(&fdinfo_dir)? {
        let entry = entry?;
        let fdinfo_path = entry.path();

        let fd_num: usize = fdinfo_path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // skip fds known to be unrelated to both GPU and NPU
        let cached_non_gpu = cache.non_gpu_fdinfos_contains(pid, fd_num);
        let cached_non_npu = cache.non_npu_fdinfos_contains(pid, fd_num);

        if cached_non_gpu && cached_non_npu {
            // recheck if the symlink target has changed to a GPU/NPU device
            let target = resolve_symlink(pid, fd_num, &fd_dir, cache);
            if let Some(ref t) = target {
                let s = t.to_string_lossy();
                if s.contains("/dev/dri/") || s.contains("/dev/accel/") {
                    cache.non_gpu_fdinfos_remove(pid, fd_num);
                    cache.non_npu_fdinfos_remove(pid, fd_num);
                } else {
                    trace!("fd {fd_num} not GPU/NPU-related (target: {t:?}), skipping");
                    continue;
                }
            } else {
                continue;
            }
        }

        // deduplicate fds pointing to the same underlying file.
        let target = resolve_symlink(pid, fd_num, &fd_dir, cache);
        if let Some(ref t) = target {
            if let Some(&first) = seen_targets.get(t) {
                trace!("fd {fd_num} aliases fd {first} (target {t:?}), skipping");
                continue;
            }
            seen_targets.insert(t.clone(), fd_num);
        }

        let content = match read_parsed::<String>(&fdinfo_path) {
            Ok(c) => c,
            Err(_) => {
                trace!("couldn't read {fdinfo_path:?}, caching as non-GPU/NPU");
                cache.non_gpu_fdinfos_insert(pid, fd_num);
                cache.non_npu_fdinfos_insert(pid, fd_num);
                continue;
            }
        };

        out.push(Fdinfo::parse(pid, fd_num, &content));
    }

    Ok(out)
}

fn resolve_symlink(
    pid: libc::pid_t,
    fd_num: usize,
    fd_dir: &Path,
    cache: &mut ProcessDataCache,
) -> Option<PathBuf> {
    if let Some(cached) = cache.symlink_cache_get(pid, fd_num) {
        return Some(cached.clone());
    }
    let target = std::fs::read_link(fd_dir.join(fd_num.to_string())).ok()?;
    cache.symlink_cache_insert(pid, fd_num, target.clone());
    Some(target)
}

pub fn extract_gpu_usage(fdinfo: &Fdinfo) -> Result<(GpuIdentifier, GpuUsageStats)> {
    let driver = fdinfo
        .fields
        .get(DRM_DRIVER)
        .ok_or_else(|| anyhow::anyhow!("no drm-driver field"))?;

    let identifier = fdinfo
        .fields
        .get(DRM_PDEV)
        .and_then(|s| s.parse::<PciSlot>().ok())
        .map(GpuIdentifier::PciSlot)
        .unwrap_or_default();

    let stats = match driver.as_str() {
        "amdgpu" => GpuUsageStats::AmdgpuStats {
            gfx_ns: fdinfo.sum_ns(&["drm-engine-compute", "drm-engine-gfx"]),
            enc_ns: fdinfo.sum_ns(&["drm-engine-enc"]),
            dec_ns: fdinfo.sum_ns(&["drm-engine-dec"]),
            mem_bytes: fdinfo.sum_kib_as_bytes(&["drm-memory-gtt", "drm-memory-vram"]),
        },
        "i915" => GpuUsageStats::I915Stats {
            gfx_ns: fdinfo.sum_ns(&["drm-engine-render"]),
            video_ns: fdinfo.sum_ns(&["drm-engine-video"]),
        },
        "v3d" => GpuUsageStats::V3dStats {
            gfx_ns: fdinfo.sum_ns(&["drm-engine-render"]),
            mem_bytes: fdinfo.sum_kib_as_bytes(&["drm-total-memory"]),
        },
        "xe" => GpuUsageStats::XeStats {
            gfx_cycles: fdinfo.sum_units(&["drm-cycles-rcs"]),
            gfx_total_cycles: fdinfo.sum_units(&["drm-total-cycles-rcs"]),
            compute_cycles: fdinfo.sum_units(&["drm-cycles-ccs"]),
            compute_total_cycles: fdinfo.sum_units(&["drm-total-cycles-ccs"]),
            video_cycles: fdinfo.sum_units(&["drm-cycles-vcs"]),
            video_total_cycles: fdinfo.sum_units(&["drm-total-cycles-vcs"]),
            mem_bytes: fdinfo.sum_kib_as_bytes(&["drm-total-gtt", "drm-total-vram0"]),
        },
        _ => bail!("unsupported GPU driver: {driver}"),
    };

    Ok((identifier, stats))
}

pub fn extract_npu_usage(fdinfo: &Fdinfo) -> Result<(PciSlot, NpuUsageStats)> {
    let driver = fdinfo
        .fields
        .get(DRM_DRIVER)
        .ok_or_else(|| anyhow::anyhow!("no drm-driver field"))?;

    let slot = fdinfo
        .fields
        .get(DRM_PDEV)
        .and_then(|s| s.parse::<PciSlot>().ok())
        .unwrap_or_default();

    let stats = match driver.as_str() {
        "amdxdna_accel_driver" => NpuUsageStats::AmdxdnaStats {
            usage_ns: fdinfo.sum_ns(&["drm-engine-npu-amdxdna"]),
            mem_bytes: fdinfo.sum_kib_as_bytes(&["drm-total-memory"]),
        },
        _ => bail!("unsupported NPU driver: {driver}"),
    };

    Ok((slot, stats))
}
