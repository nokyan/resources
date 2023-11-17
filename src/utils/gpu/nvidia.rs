use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};

use std::path::PathBuf;

use pci_ids::Device;

use crate::utils::NVML;

use super::GpuImpl;

#[derive(Debug, Default, Clone)]

pub struct NvidiaGpu {
    pub device: Option<&'static Device>,
    pub pci_slot: String,
    pub driver: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
}

impl NvidiaGpu {
    pub fn new(
        device: Option<&'static Device>,
        pci_slot: String,
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

    fn nvml_device<S: AsRef<str>>(pci_slot: S) -> Result<nvml_wrapper::Device<'static>> {
        NVML.as_ref()
            .context("unable to establish NVML connection")
            .and_then(|nvml| {
                nvml.device_by_pci_bus_id(pci_slot.as_ref())
                    .context("failed to get GPU through NVML with PCI slot")
            })
    }
}

#[async_trait]
impl GpuImpl for NvidiaGpu {
    fn device(&self) -> Option<&'static Device> {
        self.device
    }

    fn pci_slot(&self) -> String {
        self.pci_slot.clone()
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
        let nvml_answer = Self::nvml_device(self.pci_slot())
            .and_then(|dev| dev.name().context("unable to get name through NVML"));

        if let Ok(name) = nvml_answer {
            Ok(name)
        } else {
            self.drm_name()
        }
    }

    async fn usage(&self) -> Result<isize> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.utilization_rates()
                .context("unable to get utilization rates through NVML")
        });

        if let Ok(rates) = nvml_answer {
            Ok(rates.gpu as isize)
        } else {
            self.drm_usage().await
        }
    }

    async fn encode_usage(&self) -> Result<isize> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.encoder_utilization()
                .context("unable to get utilization rates through NVML")
        });

        if let Ok(rates) = nvml_answer {
            Ok(rates.utilization as isize)
        } else {
            bail!("encode usage not implemented for NVIDIA not using the nvidia driver")
        }
    }

    async fn decode_usage(&self) -> Result<isize> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.decoder_utilization()
                .context("unable to get utilization rates through NVML")
        });

        if let Ok(rates) = nvml_answer {
            Ok(rates.utilization as isize)
        } else {
            bail!("decode usage not implemented for NVIDIA not using the nvidia driver")
        }
    }

    async fn used_vram(&self) -> Result<isize> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.memory_info()
                .context("unable to get memory info through NVML")
        });

        if let Ok(memory) = nvml_answer {
            Ok(memory.used as isize)
        } else {
            self.drm_used_vram().await
        }
    }

    async fn total_vram(&self) -> Result<isize> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.memory_info()
                .context("unable to get memory info through NVML")
        });

        if let Ok(rates) = nvml_answer {
            Ok(rates.total as isize)
        } else {
            self.drm_total_vram().await
        }
    }

    async fn temperature(&self) -> Result<f64> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.temperature(TemperatureSensor::Gpu)
                .context("unable to get temperatures through NVML")
        });

        if let Ok(temp) = nvml_answer {
            Ok(temp as f64)
        } else {
            self.hwmon_temperature().await
        }
    }

    async fn power_usage(&self) -> Result<f64> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.power_usage()
                .context("unable to get temperatures through NVML")
        });

        if let Ok(power_usage) = nvml_answer {
            Ok((power_usage as f64) / 1000.0)
        } else {
            self.hwmon_power_usage().await
        }
    }

    async fn core_frequency(&self) -> Result<f64> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.clock_info(Clock::Graphics)
                .context("unable to get temperatures through NVML")
        });

        if let Ok(frequency) = nvml_answer {
            Ok((frequency as f64) * 1_000_000.0)
        } else {
            self.hwmon_core_frequency().await
        }
    }

    async fn vram_frequency(&self) -> Result<f64> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.clock_info(Clock::Memory)
                .context("unable to get temperatures through NVML")
        });

        if let Ok(frequency) = nvml_answer {
            Ok((frequency as f64) * 1_000_000.0)
        } else {
            self.hwmon_vram_frequency().await
        }
    }

    async fn power_cap(&self) -> Result<f64> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.power_management_limit()
                .context("unable to get temperatures through NVML")
        });

        if let Ok(cap) = nvml_answer {
            Ok((cap as f64) / 1000.0)
        } else {
            self.hwmon_power_usage().await
        }
    }

    async fn power_cap_max(&self) -> Result<f64> {
        let nvml_answer = Self::nvml_device(self.pci_slot()).and_then(|dev| {
            dev.power_management_limit_constraints()
                .context("unable to get temperatures through NVML")
        });

        if let Ok(constraints) = nvml_answer {
            Ok((constraints.max_limit as f64) / 1000.0)
        } else {
            self.hwmon_power_cap_max().await
        }
    }
}
