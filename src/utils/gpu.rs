use anyhow::{anyhow, Context, Result};
use nvml_wrapper::{
    enum_wrappers::device::{Clock, TemperatureSensor},
    Nvml,
};
use once_cell::sync::OnceCell;

use std::{
    collections::HashMap,
    convert::TryInto,
    fs,
    path::{Path, PathBuf},
};

use glob::glob;
use pci_ids::Device;

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
    pub fn get_gpus() -> Result<Vec<GPU>> {
        let mut gpu_vec: Vec<GPU> = Vec::new();
        for entry in glob("/sys/class/drm/card?")?.flatten() {
            let sysfs_device_path = entry.join("device");
            let mut uevent_contents: HashMap<String, String> = HashMap::new();
            let uevent_raw = fs::read_to_string(sysfs_device_path.join("uevent"))?;

            for line in uevent_raw.trim().split('\n') {
                let (k, v) = line
                    .split_once('=')
                    .context("unable to correctly read uevent file")?;
                uevent_contents.insert(k.to_owned(), v.to_owned());
            }

            let vid = u16::from_str_radix(
                uevent_contents["PCI_ID"].split(':').collect::<Vec<&str>>()[0],
                16,
            )?;
            let pid = u16::from_str_radix(
                uevent_contents["PCI_ID"].split(':').collect::<Vec<&str>>()[1],
                16,
            )?;

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
                pci_slot: uevent_contents["PCI_SLOT_NAME"].clone(),
                driver: uevent_contents["DRIVER"].clone(),
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
    pub fn get_vendor(&self) -> Result<String> {
        Ok(self.device.context("no device")?.vendor().name().to_owned())
    }

    fn read_sysfs_int<P: AsRef<Path>>(&self, file: P) -> Result<isize> {
        let path = self.sysfs_path.join(file);
        fs::read_to_string(&path)?
            .replace('\n', "")
            .parse::<isize>()
            .context(format!(
                "error parsing file {}",
                &path
                    .to_str()
                    .with_context(|| anyhow!("error transforming PathBuf to str"))?
            ))
    }

    fn read_device_int<P: AsRef<Path>>(&self, file: P) -> Result<isize> {
        let path = self.sysfs_path.join("device").join(file);
        fs::read_to_string(&path)?
            .replace('\n', "")
            .parse::<isize>()
            .context(format!(
                "error parsing file {}",
                &path
                    .to_str()
                    .with_context(|| anyhow!("error transforming PathBuf to str"))?
            ))
    }

    fn read_hwmon_int<P: AsRef<Path>>(&self, hwmon: usize, file: P) -> Result<isize> {
        let path = self.hwmon_paths[hwmon].join(file);
        fs::read_to_string(&path)?
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
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the product name of the GPU. If the nvidia driver is used,
    /// the name will be obtained using NVML, otherwise it will be obtained
    /// from the PCI ID
    pub fn get_name(&self) -> Result<String> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_name(),
                VID_INTEL => self.get_intel_name(),
                VID_NVIDIA => self.get_nvidia_name(),
                _ => self.get_pid_name(),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_gpu_usage(&self) -> Result<isize> {
        self.read_device_int("gpu_busy_percent")
    }

    fn get_intel_gpu_usage(&self) -> Result<isize> {
        Err(anyhow::anyhow!("unimplemented"))
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
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the GPU usage in percent
    pub fn get_gpu_usage(&self) -> Result<isize> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_gpu_usage(),
                VID_INTEL => self.get_intel_gpu_usage(),
                VID_NVIDIA => self.get_nvidia_gpu_usage(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_used_vram(&self) -> Result<isize> {
        self.read_device_int("mem_info_vram_used")
    }

    fn get_intel_used_vram(&self) -> Result<isize> {
        Err(anyhow::anyhow!("unimplemented"))
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
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the used VRAM in bytes
    pub fn get_used_vram(&self) -> Result<isize> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_used_vram(),
                VID_INTEL => self.get_intel_used_vram(),
                VID_NVIDIA => self.get_nvidia_used_vram(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_total_vram(&self) -> Result<isize> {
        self.read_device_int("mem_info_vram_total")
    }

    fn get_intel_total_vram(&self) -> Result<isize> {
        Err(anyhow::anyhow!("unimplemented"))
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
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the total VRAM in bytes
    pub fn get_total_vram(&self) -> Result<isize> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_total_vram(),
                VID_INTEL => self.get_intel_total_vram(),
                VID_NVIDIA => self.get_nvidia_total_vram(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_gpu_temp(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "temp1_input")? as f64 / 1000.0)
    }

    fn get_intel_gpu_temp(&self) -> Result<f64> {
        Err(anyhow::anyhow!("unimplemented"))
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
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the GPU temperature in °C
    pub fn get_gpu_temp(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_gpu_temp(),
                VID_INTEL => self.get_intel_gpu_temp(),
                VID_NVIDIA => self.get_nvidia_gpu_temp(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_power_usage(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "power1_average")? as f64 / 1000000.0)
    }

    fn get_intel_power_usage(&self) -> Result<f64> {
        Err(anyhow::anyhow!("unimplemented"))
    }

    fn get_nvidia_power_usage(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev.power_usage().context("failed to get power usage")? as f64 / 1000.0);
        }
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the GPU power usage in W
    pub fn get_power_usage(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_power_usage(),
                VID_INTEL => self.get_intel_power_usage(),
                VID_NVIDIA => self.get_nvidia_power_usage(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_gpu_speed(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "freq1_input")? as f64)
    }

    fn get_intel_gpu_speed(&self) -> Result<f64> {
        Ok(self.read_sysfs_int("gt_cur_freq_mhz")? as f64 * 1000000.0)
    }

    fn get_nvidia_gpu_speed(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev
                .clock_info(Clock::Graphics)
                .context("failed to get clock info")? as f64
                * 1000000.0);
        }
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the GPU clockspeed (typically the graphics part) in Hz
    pub fn get_gpu_speed(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_gpu_speed(),
                VID_INTEL => self.get_intel_gpu_speed(),
                VID_NVIDIA => self.get_nvidia_gpu_speed(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_vram_speed(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "freq2_input")? as f64)
    }

    fn get_intel_vram_speed(&self) -> Result<f64> {
        Err(anyhow::anyhow!("unimplemented"))
    }

    fn get_nvidia_vram_speed(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev
                .clock_info(Clock::Memory)
                .context("failed to get clock info")? as f64
                * 1000000.0);
        }
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the VRAM speed in Hz
    pub fn get_vram_speed(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_vram_speed(),
                VID_INTEL => self.get_intel_vram_speed(),
                VID_NVIDIA => self.get_nvidia_vram_speed(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_power_cap(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "power1_cap")? as f64 / 1000000.0)
    }

    fn get_intel_power_cap(&self) -> Result<f64> {
        Err(anyhow::anyhow!("unimplemented"))
    }

    fn get_nvidia_power_cap(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev
                .power_management_limit_default()
                .context("failed to get power cap info")? as f64
                / 1000.0);
        }
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the current power cap in W
    pub fn get_power_cap(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_power_cap(),
                VID_INTEL => self.get_intel_power_cap(),
                VID_NVIDIA => self.get_nvidia_power_cap(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }

    fn get_amd_power_cap_max(&self) -> Result<f64> {
        Ok(self.read_hwmon_int(0, "power1_cap_max")? as f64 / 1000000.0)
    }

    fn get_intel_power_cap_max(&self) -> Result<f64> {
        Err(anyhow::anyhow!("unimplemented"))
    }

    fn get_nvidia_power_cap_max(&self) -> Result<f64> {
        if let Ok(nv) = NVML.get_or_try_init(Nvml::init) {
            let dev = nv
                .device_by_pci_bus_id(self.pci_slot.clone())
                .context("failed to get GPU by PCI bus")?;
            return Ok(dev
                .power_management_limit_constraints()
                .context("failed to get max power cap info")?
                .max_limit as f64
                / 1000.0);
        }
        Err(anyhow::anyhow!(
            "no NVML connection, nouveau not implemented yet"
        ))
    }

    /// Returns the max power cap in W
    pub fn get_power_cap_max(&self) -> Result<f64> {
        if let Some(dev) = self.device {
            return match dev.vendor().id() {
                VID_AMD => self.get_amd_power_cap_max(),
                VID_INTEL => self.get_intel_power_cap_max(),
                VID_NVIDIA => self.get_nvidia_power_cap_max(),
                _ => Err(anyhow::anyhow!("unimplemented")),
            };
        }
        Err(anyhow::anyhow!("no device"))
    }
}
