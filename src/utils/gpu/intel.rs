use anyhow::{Result, bail};
use process_data::gpu_usage::GpuIdentifier;
use strum_macros::{Display, EnumString};

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::utils::{pci::Device, read_parsed};

use super::GpuImpl;

pub const DRIVER_NAMES: &[&str] = &["i915", "xe"];

#[derive(Debug, Clone, Display, EnumString, PartialEq, PartialOrd, Eq, Ord)]
#[strum(ascii_case_insensitive)]
#[strum(serialize_all = "lowercase")]
pub enum IntelGpuDriver {
    I915,
    Xe,
    #[strum(transparent)]
    #[strum(default)]
    Other(String),
}

impl Default for IntelGpuDriver {
    fn default() -> Self {
        Self::Other(String::new())
    }
}

#[derive(Debug, Clone)]

pub struct IntelGpu {
    pub device: Option<&'static Device>,
    pub gpu_identifier: GpuIdentifier,
    pub driver: IntelGpuDriver,
    driver_string: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
    /*
    // for some reason intel states used energy in joules instead of power in watts…
    last_energy_usage: Cell<u64>,
    last_energy_usage_timestamp: Cell<f64>,*/
}

impl IntelGpu {
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
            driver: IntelGpuDriver::from_str(&driver).unwrap_or_default(),
            driver_string: driver,
            sysfs_path,
            first_hwmon_path,
            /*last_energy_usage: Cell::new(0),
            last_energy_usage_timestamp: Cell::new(0.0),*/
        }
    }
}

impl GpuImpl for IntelGpu {
    fn device(&self) -> Option<&'static Device> {
        self.device
    }

    fn gpu_identifier(&self) -> GpuIdentifier {
        self.gpu_identifier
    }

    fn driver(&self) -> &str {
        &self.driver_string
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
        bail!("encode usage not implemented for Intel")
    }

    fn decode_usage(&self) -> Result<f64> {
        bail!("decode usage not implemented for Intel")
    }

    fn combined_media_engine(&self) -> Result<bool> {
        Ok(true)
    }

    fn used_vram(&self) -> Result<u64> {
        self.drm_used_vram().map(|usage| usage as u64)
    }

    fn total_vram(&self) -> Result<u64> {
        self.drm_total_vram().map(|usage| usage as u64)
    }

    fn temperature(&self) -> Result<f64> {
        match self.driver {
            IntelGpuDriver::Xe => read_parsed::<f64>(self.hwmon_path()?.join("temp2_input"))
                .map(|millicelsius| millicelsius / 1000.0),
            _ => self.hwmon_temperature(),
        }
    }

    /// For Intel GPUs, this returns the average power usage since last time this function was called.
    /// First call will always return Err
    fn power_usage(&self) -> Result<f64> {
        /*
        TODO: Get this working properly (or wait until xe gets its act together)
        let new_energy = read_parsed::<u64>(self.hwmon_path()?.join("energy1_input"))
            .or_else(|_| read_parsed::<u64>(self.hwmon_path()?.join("energy2_input")))
            .map(|microjoules| microjoules.saturating_div(1_000_000))?;
        let new_timestamp = unix_as_secs_f64();
        let old_energy = self.last_energy_usage.get();
        let old_timestamp = self.last_energy_usage_timestamp.get();

        self.last_energy_usage.set(new_energy);
        self.last_energy_usage_timestamp.set(new_timestamp);

        if self.last_energy_usage_timestamp.get() == 0.0 {
            bail!("first check")
        }

        let energy_delta = (new_energy.saturating_sub(old_energy)) as f64;
        let timestamp_delta = new_timestamp - old_timestamp;
        Ok(energy_delta / timestamp_delta)*/
        self.hwmon_power_usage()
    }

    fn core_frequency(&self) -> Result<f64> {
        match self.driver {
            IntelGpuDriver::Xe => Ok(read_parsed::<f64>(
                self.sysfs_path().join("device/tile0/gt0/freq0/cur_freq"),
            )? * 1_000_000.0),
            _ => Ok(read_parsed::<f64>(self.sysfs_path().join("gt_cur_freq_mhz"))? * 1_000_000.0),
        }
    }

    fn vram_frequency(&self) -> Result<f64> {
        self.hwmon_vram_frequency()
    }

    fn power_cap(&self) -> Result<f64> {
        /*
        TODO: Get this working properly (or wait until xe gets its act together)
        match self.driver {
            IntelGpuDriver::I915 => read_parsed::<f64>(self.hwmon_path()?.join("power1_max"))
                .or_else(|_| read_parsed::<f64>(self.hwmon_path()?.join("power1_crit")))
                .or_else(|_| read_parsed::<f64>(self.hwmon_path()?.join("power1_rated_max")))
                .map(|microwatts| microwatts / 1_000_000.0),
            IntelGpuDriver::Xe => read_parsed::<f64>(self.hwmon_path()?.join("power2_max"))
                .or_else(|_| read_parsed::<f64>(self.hwmon_path()?.join("power2_crit")))
                .or_else(|_| read_parsed::<f64>(self.hwmon_path()?.join("power2_rated_max")))
                .map(|microwatts| microwatts / 1_000_000.0),
            _ => self.hwmon_temperature(),
        }*/
        self.hwmon_power_cap()
    }

    fn power_cap_max(&self) -> Result<f64> {
        self.hwmon_power_cap_max()
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::IntelGpuDriver;
    use pretty_assertions::assert_eq;

    const I915_NAME: &str = "i915";
    const XE_NAME: &str = "xe";
    const OTHER_NAME: &str = "other_driver";

    #[test]
    fn i915_driver_from_str() {
        let driver = IntelGpuDriver::from_str(I915_NAME).unwrap();

        assert_eq!(driver, IntelGpuDriver::I915);
    }

    #[test]
    fn xe_driver_from_str() {
        let driver = IntelGpuDriver::from_str(XE_NAME).unwrap();

        assert_eq!(driver, IntelGpuDriver::Xe);
    }

    #[test]
    fn other_driver_from_str() {
        let driver = IntelGpuDriver::from_str(OTHER_NAME).unwrap();

        assert_eq!(driver, IntelGpuDriver::Other(OTHER_NAME.to_string()));
    }

    #[test]
    fn i915_driver_to_str() {
        assert_eq!(IntelGpuDriver::I915.to_string(), I915_NAME);
    }

    #[test]
    fn xe_driver_to_str() {
        assert_eq!(IntelGpuDriver::Xe.to_string(), XE_NAME);
    }

    #[test]
    fn other_driver_to_str() {
        assert_eq!(
            IntelGpuDriver::Other(OTHER_NAME.to_string()).to_string(),
            OTHER_NAME
        );
    }
}
