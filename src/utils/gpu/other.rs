use anyhow::{bail, Result};
use process_data::pci_slot::PciSlot;

use std::path::PathBuf;

use crate::utils::pci::Device;

use super::GpuImpl;

#[derive(Debug, Clone, Default)]

pub struct OtherGpu {
    pub device: Option<&'static Device>,
    pub pci_slot: PciSlot,
    pub driver: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
}

impl OtherGpu {
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
        }
    }
}

impl GpuImpl for OtherGpu {
    fn device(&self) -> Option<&'static Device> {
        self.device
    }

    fn pci_slot(&self) -> PciSlot {
        self.pci_slot
    }

    fn driver(&self) -> String {
        self.driver.clone()
    }

    fn sysfs_path(&self) -> PathBuf {
        self.sysfs_path.clone()
    }

    fn first_hwmon(&self) -> Option<PathBuf> {
        self.first_hwmon_path.clone()
    }

    fn name(&self) -> Result<String> {
        self.drm_name()
    }

    fn usage(&self) -> Result<isize> {
        self.drm_usage()
    }

    fn encode_usage(&self) -> Result<isize> {
        bail!("encode usage not implemented for other")
    }

    fn decode_usage(&self) -> Result<isize> {
        bail!("decode usage not implemented for other")
    }

    fn used_vram(&self) -> Result<isize> {
        self.drm_used_vram()
    }

    fn total_vram(&self) -> Result<isize> {
        self.drm_total_vram()
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

    fn vram_frequency(&self) -> Result<f64> {
        self.hwmon_vram_frequency()
    }

    fn power_cap(&self) -> Result<f64> {
        self.hwmon_power_cap()
    }

    fn power_cap_max(&self) -> Result<f64> {
        self.hwmon_power_cap_max()
    }
}
