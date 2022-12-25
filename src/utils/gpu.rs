use anyhow::{anyhow, bail, Context, Result};
use nvml_wrapper::{
    enum_wrappers::device::{Clock, TemperatureSensor},
    Nvml,
};
use once_cell::sync::OnceCell;

use std::{
    collections::HashMap,
    convert::TryInto,
    path::{Path, PathBuf},
};

use glob::glob;
use pci_ids::Device;

use crate::i18n::i18n;

const VID_AMD: u16 = 4098;
const VID_INTEL: u16 = 32902;
const VID_NVIDIA: u16 = 4318;

static NVML: OnceCell<Nvml> = OnceCell::new();

#[derive(Debug, Clone, Default)]
pub struct GPU {
    pub device: Option<&'static Device>,
    pub pci_slot: String,
    pub driver: String,
    sysfs_path: PathBuf,
    hwmon_paths: Vec<PathBuf>,
}

impl GPU {
    /// Returns a `Vec` of all GPUs currently found in the system.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems detecting
    /// the GPUs in the system
    pub async fn get_gpus() -> Result<Vec<GPU>> {
        let mut gpu_vec: Vec<GPU> = Vec::new();
        for entry in glob("/sys/class/drm/card?")?.flatten() {
            let sysfs_device_path = entry.join("device");
            let mut uevent_contents: HashMap<String, String> = HashMap::new();
            let uevent_raw =
                async_std::fs::read_to_string(sysfs_device_path.join("uevent")).await?;

            for line in uevent_raw.trim().split('\n') {
                let (k, v) = line
                    .split_once('=')
                    .context("unable to correctly read uevent file")?;
                uevent_contents.insert(k.to_owned(), v.to_owned());
            }

            let mut vid: u16 = 0;
            let mut pid: u16 = 0;

            if let Some(pci_line) = uevent_contents.get("PCI_ID") {
                let split = pci_line.split(':').collect::<Vec<&str>>();
                vid = u16::from_str_radix(split[0], 16)?;
                pid = u16::from_str_radix(split[1], 16)?;
            }

            let mut hwmon_vec: Vec<PathBuf> = Vec::new();
            for hwmon in glob(&format!(
                "{}/hwmon/hwmon?",
                sysfs_device_path
                    .to_str()
                    .with_context(|| anyhow!("error transforming PathBuf to str"))?
            ))?
            .flatten()
            {
                hwmon_vec.push(hwmon);
            }

            gpu_vec.push(GPU {
                device: Device::from_vid_pid(vid, pid),
                pci_slot: uevent_contents
                    .get("PCI_SLOT_NAME")
                    .map_or_else(|| i18n("N/A"), std::string::ToString::to_string),
                driver: uevent_contents
                    .get("DRIVER")
                    .map_or_else(|| i18n("N/A"), std::string::ToString::to_string),
                sysfs_path: entry,
                hwmon_paths: hwmon_vec,
            });
        }
        Ok(gpu_vec)
    }

    fn get_pid_name(&self) -> Result<String> {
        Ok(self.device.context("no device")?.name().to_owned())
    }

    /// Returns the Vendor name using the GPU's Vendor ID
    ///
    /// # Errors
    ///
    /// Will return `Err` if the vendor is unknown
    /// in the PCI IDs database
    pub fn get_vendor(&self) -> Result<String> {
        Ok(self.device.context("no device")?.vendor().name().to_owned())
    }

    async fn read_sysfs_int<P: AsRef<Path>>(&self, file: P) -> Result<isize> {
        let path = self.sysfs_path.join(file);
        async_std::fs::read_to_string(&path)
            .await?
            .replace('\n', "")
            .parse::<isize>()
            .context(format!(
                "error parsing file {}",
                &path
                    .to_str()
                    .with_context(|| anyhow!("error transforming PathBuf to str"))?
            ))
    }

    async fn read_device_int<P: AsRef<Path>>(&self, file: P) -> Result<isize> {
        let path = self.sysfs_path.join("device").join(file);
        async_std::fs::read_to_string(&path)
            .await?
            .replace('\n', "")
            .parse::<isize>()
            .context(format!(
                "error parsing file {}",
                &path
                    .to_str()
                    .with_context(|| anyhow!("error transforming PathBuf to str"))?
            ))
    }

    async fn read_hwmon_int<P: AsRef<Path>>(&self, hwmon: usize, file: P) -> Result<isize> {
        let path = self.hwmon_paths[hwmon].join(file);
        async_std::fs::read_to_string(&path)
            .await?
            .replace('\n', "")
            .parse::<isize>()
            .context(format!(
                "error parsing file {}",
                &path
                    .to_str()
                    .with_context(|| anyhow!("error transforming PathBuf to str"))?
            ))
    }

    fn get_amd_name(&self) -> Result<String> {
        self.get_pid_name()
    }

    fn get_intel_name(&self) -> Result<String> {
        self.get_pid_name()
    }

    fn get_nvidia_name(&self) -> Result<String> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return dev.name().context("failed to get utilization rates");
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the product name of the GPU. If the nvidia driver is used,
    /// the name will be obtained using NVML, otherwise it will be obtained
    /// from the PCI ID
    ///
    /// # Errors
    ///
    /// Will return `Err` if there's no name either exposed through a driver,
    /// exposed through a sysfs file or findable in the PCI IDs database.
    pub fn get_name(&self) -> Result<String> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_name(),
                VID_INTEL => self.get_intel_name(),
                VID_NVIDIA => self.get_nvidia_name(),
                _ => self.get_pid_name(),
            };
        }
        bail!("no device")
    }

    async fn get_amd_gpu_usage(&self) -> Result<isize> {
        self.read_device_int("gpu_busy_percent").await
    }

    fn get_intel_gpu_usage(&self) -> Result<isize> {
        bail!("unimplemented")
    }

    fn get_nvidia_gpu_usage(&self) -> Result<isize> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev
                .utilization_rates()
                .context("failed to get utilization rates")?
                .gpu
                .try_into()?);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the GPU usage in percent
    ///
    /// # Errors
    ///
    /// Will return `Err` if the GPU usage
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_gpu_usage(&self) -> Result<isize> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_gpu_usage().await,
                VID_INTEL => self.get_intel_gpu_usage(),
                VID_NVIDIA => self.get_nvidia_gpu_usage(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }

    async fn get_amd_used_vram(&self) -> Result<isize> {
        self.read_device_int("mem_info_vram_used").await
    }

    fn get_intel_used_vram(&self) -> Result<isize> {
        bail!("unimplemented")
    }

    fn get_nvidia_used_vram(&self) -> Result<isize> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev
                .memory_info()
                .context("failed to get memory info")?
                .used
                .try_into()?);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the used VRAM in bytes
    ///
    ///
    /// # Errors
    ///
    /// Will return `Err` if the used amount of VRAM
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_used_vram(&self) -> Result<isize> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_used_vram().await,
                VID_INTEL => self.get_intel_used_vram(),
                VID_NVIDIA => self.get_nvidia_used_vram(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }

    async fn get_amd_total_vram(&self) -> Result<isize> {
        self.read_device_int("mem_info_vram_total").await
    }

    fn get_intel_total_vram(&self) -> Result<isize> {
        bail!("unimplemented")
    }

    fn get_nvidia_total_vram(&self) -> Result<isize> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev
                .memory_info()
                .context("failed to get memory info")?
                .total
                .try_into()?);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the total VRAM in bytes
    ///
    /// # Errors
    ///
    /// Will return `Err` if the VRAM size
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_total_vram(&self) -> Result<isize> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_total_vram().await,
                VID_INTEL => self.get_intel_total_vram(),
                VID_NVIDIA => self.get_nvidia_total_vram(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }

    async fn get_amd_gpu_temp(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "temp1_input").await? as f64 / 1000.0)
    }

    fn get_intel_gpu_temp(&self) -> Result<f64> {
        bail!("unimplemented")
    }

    fn get_nvidia_gpu_temp(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev
                .temperature(TemperatureSensor::Gpu)
                .context("failed to get temperature info")?
                .try_into()?);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the GPU temperature in °C
    ///
    /// # Errors
    ///
    /// Will return `Err` if the temperature
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_gpu_temp(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_gpu_temp().await,
                VID_INTEL => self.get_intel_gpu_temp(),
                VID_NVIDIA => self.get_nvidia_gpu_temp(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }

    async fn get_amd_power_usage(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "power1_average").await? as f64 / 1_000_000.0)
    }

    fn get_intel_power_usage(&self) -> Result<f64> {
        bail!("unimplemented")
    }

    fn get_nvidia_power_usage(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(f64::from(dev.power_usage().context("failed to get power usage")?) / 1000.0);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the GPU power usage in Watts
    ///
    /// # Errors
    ///
    /// Will return `Err` if the power usage
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_power_usage(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_power_usage().await,
                VID_INTEL => self.get_intel_power_usage(),
                VID_NVIDIA => self.get_nvidia_power_usage(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }

    async fn get_amd_gpu_speed(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "freq1_input").await? as f64)
    }

    async fn get_intel_gpu_speed(&self) -> Result<f64> {
        Ok(self.read_sysfs_int("gt_cur_freq_mhz").await? as f64 * 1_000_000.0)
    }

    fn get_nvidia_gpu_speed(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(f64::from(
                dev.clock_info(Clock::Graphics)
                    .context("failed to get clock info")?,
            ) * 1_000_000.0);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the GPU clockspeed (typically the 3-D
    /// graphics part) in Hz
    ///
    /// # Errors
    ///
    /// Will return `Err` if the clockspeed
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_gpu_speed(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_gpu_speed().await,
                VID_INTEL => self.get_intel_gpu_speed().await,
                VID_NVIDIA => self.get_nvidia_gpu_speed(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }

    async fn get_amd_vram_speed(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "freq2_input").await? as f64)
    }

    fn get_intel_vram_speed(&self) -> Result<f64> {
        bail!("unimplemented")
    }

    fn get_nvidia_vram_speed(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(f64::from(
                dev.clock_info(Clock::Memory)
                    .context("failed to get clock info")?,
            ) * 1_000_000.0);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the VRAM speed in Hz
    ///
    /// # Errors
    ///
    /// Will return `Err` if the VRAM speed
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_vram_speed(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_vram_speed().await,
                VID_INTEL => self.get_intel_vram_speed(),
                VID_NVIDIA => self.get_nvidia_vram_speed(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }

    async fn get_amd_power_cap(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "power1_cap").await? as f64 / 1_000_000.0)
    }

    fn get_intel_power_cap(&self) -> Result<f64> {
        bail!("unimplemented")
    }

    fn get_nvidia_power_cap(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(f64::from(
                dev.power_management_limit_default()
                    .context("failed to get power cap info")?,
            ) / 1000.0);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the current power cap in Watts
    ///
    /// # Errors
    ///
    /// Will return `Err` if the current power cap
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_power_cap(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_power_cap().await,
                VID_INTEL => self.get_intel_power_cap(),
                VID_NVIDIA => self.get_nvidia_power_cap(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }

    async fn get_amd_power_cap_max(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "power1_cap_max").await? as f64 / 1_000_000.0)
    }

    fn get_intel_power_cap_max(&self) -> Result<f64> {
        bail!("unimplemented")
    }

    fn get_nvidia_power_cap_max(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(f64::from(
                dev.power_management_limit_constraints()
                    .context("failed to get max power cap info")?
                    .max_limit,
            ) / 1000.0);
        }
        bail!("no NVML connection, nouveau not implemented yet")
    }

    /// Returns the max power cap in Watts
    ///
    /// # Errors
    ///
    /// Will return `Err` if the max power cap
    /// is for some reason unreadable or is simply
    /// not exposed
    pub async fn get_power_cap_max(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_power_cap_max().await,
                VID_INTEL => self.get_intel_power_cap_max(),
                VID_NVIDIA => self.get_nvidia_power_cap_max(),
                _ => bail!("unimplemented"),
            };
        }
        bail!("no device")
    }
}
