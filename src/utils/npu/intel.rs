use anyhow::Result;
use process_data::{pci_slot::PciSlot, unix_as_secs_f64};

use std::{
    cell::Cell,
    path::{Path, PathBuf},
};

use crate::utils::{pci::Device, read_parsed};

use super::NpuImpl;

#[derive(Debug, Clone, Default)]

pub struct IntelNpu {
    pub device: Option<&'static Device>,
    pub pci_slot: PciSlot,
    pub driver: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
    last_busy_time_us: Cell<usize>,
    last_busy_time_timestamp: Cell<f64>,
}

impl IntelNpu {
    pub fn new(
        device: Option<&'static Device>,
        pci_slot: PciSlot,
        driver: String,
        sysfs_path: PathBuf,
        first_hwmon_path: Option<PathBuf>,
    ) -> Self {
        Self {
            device,
            pci_slot,
            driver,
            sysfs_path,
            first_hwmon_path,
            last_busy_time_us: Cell::default(),
            last_busy_time_timestamp: Cell::default(),
        }
    }
}

impl NpuImpl for IntelNpu {
    fn device(&self) -> Option<&'static Device> {
        self.device
    }

    fn pci_slot(&self) -> PciSlot {
        self.pci_slot
    }

    fn driver(&self) -> &str {
        &self.driver
    }

    fn sysfs_path(&self) -> &Path {
        &self.sysfs_path
    }

    fn first_hwmon(&self) -> Option<&Path> {
        self.first_hwmon_path.as_deref()
    }

    fn name(&self) -> Result<String> {
        self.drm_name()
    }

    fn usage(&self) -> Result<f64> {
        let last_timestamp = self.last_busy_time_timestamp.get();
        let last_busy_time = self.last_busy_time_us.get();

        let new_timestamp = unix_as_secs_f64();
        let new_busy_time = read_parsed(self.sysfs_path().join("device/npu_busy_time_us"))?;

        self.last_busy_time_timestamp.set(new_timestamp);
        self.last_busy_time_us.set(new_busy_time);

        let delta_timestamp = new_timestamp - last_timestamp;
        let delta_busy_time = new_busy_time.saturating_sub(last_busy_time) as f64;

        Ok(delta_busy_time / delta_timestamp)
    }

    fn used_vram(&self) -> Result<usize> {
        self.drm_used_memory().map(|usage| usage as usize)
    }

    fn total_vram(&self) -> Result<usize> {
        self.drm_total_memory().map(|usage| usage as usize)
    }

    fn temperature(&self) -> Result<f64> {
        self.hwmon_temperature()
    }

    fn power_usage(&self) -> Result<f64> {
        self.hwmon_power_usage()
    }

    fn core_frequency(&self) -> Result<f64> {
        self.hwmon_core_frequency()
    }

    fn memory_frequency(&self) -> Result<f64> {
        self.hwmon_memory_frequency()
    }

    fn power_cap(&self) -> Result<f64> {
        self.hwmon_power_cap()
    }

    fn power_cap_max(&self) -> Result<f64> {
        self.hwmon_power_cap_max()
    }
}
