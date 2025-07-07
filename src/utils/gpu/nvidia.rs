use anyhow::{Context, Result};
use log::{debug, warn};
use nvml_wrapper::{
    Nvml,
    enum_wrappers::device::{Clock, TemperatureSensor},
    error::NvmlError,
};
use process_data::GpuIdentifier;

use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

static NVML: LazyLock<Result<Nvml, NvmlError>> = LazyLock::new(|| {
    let nvml = Nvml::init();

    if let Err(error) = nvml.as_ref() {
        warn!("Connection to NVML failed, reason: {error}");
        if *IS_FLATPAK {
            warn!(
                "This can occur when the version of the NVIDIA Flatpak runtime (org.freedesktop.Platform.GL.nvidia) \
            and the version of the natively installed NVIDIA driver do not match. Consider updating both your system \
            and Flatpak packages before opening an issue."
            );
        }
    } else {
        debug!("Successfully connected to NVML");
    }

    nvml
});

use crate::utils::{IS_FLATPAK, pci::Device};

use super::GpuImpl;

#[derive(Debug, Default, Clone)]

pub struct NvidiaGpu {
    pub device: Option<&'static Device>,
    pub gpu_identifier: GpuIdentifier,
    pub driver: String,
    pci_slot_string: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
}

impl NvidiaGpu {
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
            pci_slot_string: gpu_identifier.to_string(),
            sysfs_path,
            first_hwmon_path,
        }
    }

    fn nvml_device<S: AsRef<str>>(pci_slot: S) -> Result<nvml_wrapper::Device<'static>> {
        NVML.as_ref()
            .context("unable to establish NVML connection")
            .and_then(|nvml| {
                nvml.device_by_pci_bus_id(pci_slot.as_ref())
                    .context("failed to get GPU through NVML with PCI slot")
            })
    }
}

impl GpuImpl for NvidiaGpu {
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
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| dev.name().context("unable to get name through NVML"))
            .or_else(|_| self.drm_name())
    }

    fn usage(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.utilization_rates()
                    .context("unable to get utilization rates through NVML")
            })
            .map(|usage| f64::from(usage.gpu) / 100.0)
            .or_else(|_| self.drm_usage().map(|usage| usage as f64 / 100.0))
    }

    fn encode_usage(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.encoder_utilization()
                    .context("unable to get utilization rates through NVML")
            })
            .map(|usage| f64::from(usage.utilization) / 100.0)
            .context("encode usage not implemented for NVIDIA not using the nvidia driver")
    }

    fn decode_usage(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.decoder_utilization()
                    .context("unable to get utilization rates through NVML")
            })
            .map(|usage| f64::from(usage.utilization) / 100.0)
            .context("decode usage not implemented for NVIDIA not using the nvidia driver")
    }

    fn combined_media_engine(&self) -> Result<bool> {
        Ok(false)
    }

    fn used_vram(&self) -> Result<usize> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.memory_info()
                    .context("unable to get memory info through NVML")
            })
            .map(|memory_info| memory_info.used as usize)
            .or_else(|_| self.drm_used_vram().map(|usage| usage as usize))
    }

    fn total_vram(&self) -> Result<usize> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.memory_info()
                    .context("unable to get memory info through NVML")
            })
            .map(|memory_info| memory_info.total as usize)
            .or_else(|_| self.drm_total_vram().map(|usage| usage as usize))
    }

    fn temperature(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.temperature(TemperatureSensor::Gpu)
                    .context("unable to get temperatures through NVML")
            })
            .map(f64::from)
            .or_else(|_| self.hwmon_temperature())
    }

    fn power_usage(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.power_usage()
                    .context("unable to get power usage through NVML")
            })
            .map(|power_usage| (f64::from(power_usage)) / 1000.0)
            .or_else(|_| self.hwmon_power_usage())
    }

    fn core_frequency(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.clock_info(Clock::Graphics)
                    .context("unable to get core frequency through NVML")
            })
            .map(|frequency| (f64::from(frequency)) * 1_000_000.0)
            .or_else(|_| self.hwmon_core_frequency())
    }

    fn vram_frequency(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.clock_info(Clock::Memory)
                    .context("unable to get vram frequency through NVML")
            })
            .map(|frequency| (f64::from(frequency)) * 1_000_000.0)
            .or_else(|_| self.hwmon_vram_frequency())
    }

    fn power_cap(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.power_management_limit()
                    .context("unable to get power cap through NVML")
            })
            .map(|cap| (f64::from(cap)) / 1000.0)
            .or_else(|_| self.hwmon_power_usage())
    }

    fn power_cap_max(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.power_management_limit_constraints()
                    .context("unable to get temperatures through NVML")
            })
            .map(|constraints| (f64::from(constraints.max_limit)) / 1000.0)
            .or_else(|_| self.hwmon_power_cap_max())
    }
}
