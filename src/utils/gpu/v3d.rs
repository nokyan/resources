use anyhow::{Result, bail};
use process_data::GpuIdentifier;

use std::path::{Path, PathBuf};

use crate::utils::{pci::Device, read_sysfs};

use super::GpuImpl;

#[derive(Debug, Clone, Default)]

pub struct V3dGpu {
    pub device: Option<&'static Device>,
    pub gpu_identifier: GpuIdentifier,
    pub driver: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
}

impl V3dGpu {
    pub fn new(
        device: Option<&'static Device>,
        gpu_identifier: GpuIdentifier,
        driver: String,
        sysfs_path: PathBuf,
        first_hwmon_path: Option<PathBuf>,
    ) -> Self {
        Self {
            device,
            gpu_identifier,
            driver,
            sysfs_path,
            first_hwmon_path,
        }
    }
}

impl GpuImpl for V3dGpu {
    fn device(&self) -> Option<&'static Device> {
        self.device
    }

    fn gpu_identifier(&self) -> GpuIdentifier {
        self.gpu_identifier
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
        self.drm_usage().map(|usage| usage as f64 / 100.0)
    }

    fn encode_usage(&self) -> Result<f64> {
        bail!("encode usage not implemented for v3d")
    }

    fn decode_usage(&self) -> Result<f64> {
        bail!("decode usage not implemented for v3d")
    }

    fn combined_media_engine(&self) -> Result<bool> {
        Ok(true)
    }

    fn used_vram(&self) -> Result<usize> {
        self.drm_used_vram().map(|usage| usage as usize)
    }

    fn total_vram(&self) -> Result<usize> {
        self.drm_total_vram().map(|usage| usage as usize)
    }

    fn temperature(&self) -> Result<f64> {
        self.hwmon_temperature()
    }

    fn power_usage(&self) -> Result<f64> {
        self.hwmon_power_usage()
    }

    fn core_frequency(&self) -> Result<f64> {
        read_sysfs::<isize>(self.sysfs_path().join("gt_cur_freq_mhz"))
            .map(|freq| freq as f64 * 1_000_000.0)
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
