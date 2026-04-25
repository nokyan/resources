use log::{debug, trace, warn};
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::struct_wrappers::device::{ProcessInfo, ProcessUtilizationSample};
use nvml_wrapper::{Device, Nvml};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::LazyLock;

use crate::gpu_usage::{GpuIdentifier, GpuUsageStats, IntegerPercentage};
use crate::pci_slot::PciSlot;
use crate::unix_as_millis;

static NVML: LazyLock<Result<Nvml, NvmlError>> = LazyLock::new(|| {
    debug!("Initializing NVML…");
    Nvml::init().inspect_err(|e| warn!("Unable to connect to NVML: {e}"))
});

static NVML_DEVICES: LazyLock<Vec<(PciSlot, Device)>> = LazyLock::new(|| {
    let Ok(nvml) = NVML.as_ref() else {
        return Vec::new();
    };
    debug!("Looking for NVIDIA devices…");
    let count = nvml.device_count().unwrap_or(0);
    let mut devices = Vec::with_capacity(count as usize);
    for i in 0..count {
        if let Ok(gpu) = nvml.device_by_index(i) {
            if let Ok(bus_id) = gpu.pci_info().map(|p| p.bus_id) {
                match PciSlot::from_str(&bus_id) {
                    Ok(slot) => {
                        debug!(
                            "Found {} at {slot}",
                            gpu.name().unwrap_or_else(|_| "N/A".into())
                        );
                        devices.push((slot, gpu));
                    }
                    Err(e) => warn!("Couldn't parse PCI slot '{bus_id}': {e}"),
                }
            }
        }
    }
    devices
});

#[derive(Debug, Clone, Default)]
pub struct NvidiaState {
    util_map: HashMap<PciSlot, Vec<ProcessUtilizationSample>>,
    info_map: HashMap<PciSlot, Vec<ProcessInfo>>,
}

impl NvidiaState {
    pub fn refresh(&mut self) {
        trace!("Refreshing NVIDIA process stats…");

        self.util_map.clear();
        for (slot, gpu) in NVML_DEVICES.iter() {
            let since = unix_as_millis()
                .saturating_mul(1000)
                .saturating_sub(5_000_000);
            self.util_map.insert(
                *slot,
                gpu.process_utilization_stats(since).unwrap_or_default(),
            );
        }

        self.info_map.clear();
        for (slot, gpu) in NVML_DEVICES.iter() {
            let mut procs = gpu.running_graphics_processes().unwrap_or_default();
            procs.extend(gpu.running_compute_processes().unwrap_or_default());
            self.info_map.insert(*slot, procs);
        }
    }

    pub fn per_process_stats(&self, pid: libc::pid_t) -> BTreeMap<GpuIdentifier, GpuUsageStats> {
        trace!("Gathering NVIDIA GPU stats for pid {pid}…");
        let mut map = BTreeMap::new();
        for (slot, _) in NVML_DEVICES.iter() {
            if let Ok(stats) = self.stats_for_pid_on_gpu(pid, *slot) {
                map.insert(GpuIdentifier::PciSlot(*slot), stats);
            }
        }
        map
    }

    fn stats_for_pid_on_gpu(
        &self,
        pid: libc::pid_t,
        slot: PciSlot,
    ) -> anyhow::Result<GpuUsageStats> {
        let util_samples = self
            .util_map
            .get(&slot)
            .ok_or_else(|| anyhow::anyhow!("no stats for slot {slot}"))?;

        let (sm, enc, dec) = util_samples
            .iter()
            .filter(|s| s.pid == pid as u32)
            .map(|s| (s.sm_util, s.enc_util, s.dec_util))
            .reduce(|acc, cur| (acc.0 + cur.0, acc.1 + cur.1, acc.2 + cur.2))
            .unwrap_or_default();

        let mem_bytes: u64 = self
            .info_map
            .get(&slot)
            .ok_or_else(|| anyhow::anyhow!("no infos for slot {slot}"))?
            .iter()
            .filter(|p| p.pid == pid as u32)
            .map(|p| match p.used_gpu_memory {
                UsedGpuMemory::Used(b) => b,
                UsedGpuMemory::Unavailable => 0,
            })
            .sum();

        Ok(GpuUsageStats::NvidiaStats {
            gfx_percentage: IntegerPercentage::try_new(sm as u8)?,
            enc_percentage: IntegerPercentage::try_new(enc as u8)?,
            dec_percentage: IntegerPercentage::try_new(dec as u8)?,
            mem_bytes,
        })
    }
}
