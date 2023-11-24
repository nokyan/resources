use anyhow::{Context, Result};
use log::{debug, warn};
use nvml_wrapper::{
    enum_wrappers::device::{Clock, TemperatureSensor},
    error::NvmlError,
    Nvml,
};
use once_cell::sync::Lazy;
use process_data::pci_slot::PciSlot;

use std::path::PathBuf;

use pci_ids::Device;

static NVML: Lazy<Result<Nvml, NvmlError>> = Lazy::new(|| {
    let nvml = Nvml::init();

    if let Err(error) = nvml.as_ref() {
        warn!("Connection to NVML failed, reason: {error}");
    } else {
        debug!("Successfully connected to NVML");
    }

    nvml
});

use super::GpuImpl;

#[derive(Debug, Default, Clone)]

pub struct NvidiaGpu {
    pub device: Option<&'static Device>,
    pub pci_slot: PciSlot,
    pub driver: String,
    pci_slot_string: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
}

impl NvidiaGpu {
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
            pci_slot_string: pci_slot.to_string(),
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
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| dev.name().context("unable to get name through NVML"))
            .or_else(|_| self.drm_name())
    }

    fn usage(&self) -> Result<isize> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.utilization_rates()
                    .context("unable to get utilization rates through NVML")
            })
            .map(|usage| usage.gpu as isize)
            .or_else(|_| self.drm_usage())
    }

    fn encode_usage(&self) -> Result<isize> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.encoder_utilization()
                    .context("unable to get utilization rates through NVML")
            })
            .map(|usage| usage.utilization as isize)
            .context("encode usage not implemented for NVIDIA not using the nvidia driver")
    }

    fn decode_usage(&self) -> Result<isize> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.decoder_utilization()
                    .context("unable to get utilization rates through NVML")
            })
            .map(|usage| usage.utilization as isize)
            .context("decode usage not implemented for NVIDIA not using the nvidia driver")
    }

    fn used_vram(&self) -> Result<isize> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.memory_info()
                    .context("unable to get memory info through NVML")
            })
            .map(|memory_info| memory_info.used as isize)
            .or_else(|_| self.drm_used_vram())
    }

    fn total_vram(&self) -> Result<isize> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.memory_info()
                    .context("unable to get memory info through NVML")
            })
            .map(|memory_info| memory_info.total as isize)
            .or_else(|_| self.drm_total_vram())
    }

    fn temperature(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.temperature(TemperatureSensor::Gpu)
                    .context("unable to get temperatures through NVML")
            })
            .map(|temp| temp as f64)
            .or_else(|_| self.hwmon_temperature())
    }

    fn power_usage(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.power_usage()
                    .context("unable to get power usage through NVML")
            })
            .map(|power_usage| (power_usage as f64) / 1000.0)
            .or_else(|_| self.hwmon_power_usage())
    }

    fn core_frequency(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.clock_info(Clock::Graphics)
                    .context("unable to get core frequency through NVML")
            })
            .map(|frequency| (frequency as f64) * 1_000_000.0)
            .or_else(|_| self.hwmon_core_frequency())
    }

    fn vram_frequency(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.clock_info(Clock::Memory)
                    .context("unable to get vram frequency through NVML")
            })
            .map(|frequency| (frequency as f64) * 1_000_000.0)
            .or_else(|_| self.hwmon_vram_frequency())
    }

    fn power_cap(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.power_management_limit()
                    .context("unable to get power cap through NVML")
            })
            .map(|cap| (cap as f64) / 1000.0)
            .or_else(|_| self.hwmon_power_usage())
    }

    fn power_cap_max(&self) -> Result<f64> {
        Self::nvml_device(&self.pci_slot_string)
            .and_then(|dev| {
                dev.power_management_limit_constraints()
                    .context("unable to get temperatures through NVML")
            })
            .map(|constraints| (constraints.max_limit as f64) / 1000.0)
            .or_else(|_| self.hwmon_power_cap_max())
    }
}
