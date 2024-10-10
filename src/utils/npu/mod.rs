mod intel;
mod other;

use anyhow::{bail, Context, Result};
use log::{debug, info};
use process_data::pci_slot::PciSlot;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use glob::glob;

use crate::{i18n::i18n, utils::pci::Device};

use self::{intel::IntelNpu, other::OtherNpu};

use super::pci::Vendor;

pub const VID_AMD: u16 = 0x1002;
pub const VID_INTEL: u16 = 0x8086;
pub const VID_NVIDIA: u16 = 0x10DE;

#[derive(Debug)]
pub struct NpuData {
    pub pci_slot: PciSlot,

    pub usage_fraction: Option<f64>,

    pub total_vram: Option<usize>,
    pub used_vram: Option<usize>,

    pub clock_speed: Option<f64>,
    pub vram_speed: Option<f64>,

    pub temp: Option<f64>,

    pub power_usage: Option<f64>,
    pub power_cap: Option<f64>,
    pub power_cap_max: Option<f64>,
}

impl NpuData {
    pub fn new(npu: &Npu) -> Self {
        let pci_slot = npu.pci_slot();

        let usage_fraction = npu.usage().ok();

        let total_vram = npu.total_vram().ok();
        let used_vram = npu.used_vram().ok();

        let clock_speed = npu.core_frequency().ok();
        let vram_speed = npu.vram_frequency().ok();

        let temp = npu.temperature().ok();

        let power_usage = npu.power_usage().ok();
        let power_cap = npu.power_cap().ok();
        let power_cap_max = npu.power_cap_max().ok();

        Self {
            pci_slot,
            usage_fraction,
            total_vram,
            used_vram,
            clock_speed,
            vram_speed,
            temp,
            power_usage,
            power_cap,
            power_cap_max,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Npu {
    Intel(IntelNpu),
    Other(OtherNpu),
}

impl Default for Npu {
    fn default() -> Self {
        Npu::Other(OtherNpu::default())
    }
}

pub trait NpuImpl {
    fn device(&self) -> Option<&'static Device>;
    fn pci_slot(&self) -> PciSlot;
    fn driver(&self) -> String;
    fn sysfs_path(&self) -> PathBuf;
    fn first_hwmon(&self) -> Option<PathBuf>;

    fn name(&self) -> Result<String>;
    fn usage(&self) -> Result<f64>;
    fn used_vram(&self) -> Result<usize>;
    fn total_vram(&self) -> Result<usize>;
    fn temperature(&self) -> Result<f64>;
    fn power_usage(&self) -> Result<f64>;
    fn core_frequency(&self) -> Result<f64>;
    fn vram_frequency(&self) -> Result<f64>;
    fn power_cap(&self) -> Result<f64>;
    fn power_cap_max(&self) -> Result<f64>;

    fn read_sysfs_int<P: AsRef<Path> + std::marker::Send>(&self, file: P) -> Result<isize> {
        let path = self.sysfs_path().join(file);
        std::fs::read_to_string(&path)?
            .replace('\n', "")
            .parse::<isize>()
            .with_context(|| format!("error parsing file {}", &path.to_string_lossy()))
    }

    fn read_device_file<P: AsRef<Path> + std::marker::Send>(&self, file: P) -> Result<String> {
        let path = self.sysfs_path().join("device").join(file);
        Ok(std::fs::read_to_string(path)?.replace('\n', ""))
    }

    fn read_device_int<P: AsRef<Path> + std::marker::Send>(&self, file: P) -> Result<isize> {
        let path = self.sysfs_path().join("device").join(file);
        self.read_device_file(&path)?
            .parse::<isize>()
            .with_context(|| format!("error parsing file {}", &path.to_string_lossy()))
    }

    fn read_hwmon_int<P: AsRef<Path> + std::marker::Send>(&self, file: P) -> Result<isize> {
        let path = self.first_hwmon().context("no hwmon found")?.join(file);
        std::fs::read_to_string(&path)?
            .replace('\n', "")
            .parse::<isize>()
            .with_context(|| format!("error parsing file {}", &path.to_string_lossy()))
    }

    // These are preimplemented ways of getting information through the DRM and hwmon interface.
    // It's also used as a fallback.

    fn drm_name(&self) -> Result<String> {
        Ok(self.device().context("no device")?.name().to_owned())
    }

    fn drm_usage(&self) -> Result<isize> {
        bail!("usage fallback not implemented")
    }

    fn drm_used_vram(&self) -> Result<isize> {
        self.read_device_int("mem_info_vram_used")
    }

    fn drm_total_vram(&self) -> Result<isize> {
        self.read_device_int("mem_info_vram_total")
    }

    fn hwmon_temperature(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("temp1_input")? as f64 / 1000.0)
    }

    fn hwmon_power_usage(&self) -> Result<f64> {
        Ok(self
            .read_hwmon_int("power1_average")
            .or_else(|_| self.read_hwmon_int("power1_input"))? as f64
            / 1_000_000.0)
    }

    fn hwmon_core_frequency(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("freq1_input")? as f64)
    }

    fn hwmon_vram_frequency(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("freq2_input")? as f64)
    }

    fn hwmon_power_cap(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("power1_cap")? as f64 / 1_000_000.0)
    }

    fn hwmon_power_cap_max(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("power1_cap_max")? as f64 / 1_000_000.0)
    }
}

impl Npu {
    /// Returns a `Vec` of all NPUs currently found in the system.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems detecting
    /// the NPUs in the system
    pub fn get_npus() -> Result<Vec<Npu>> {
        debug!("Searching for NPUs…");

        let mut npu_vec: Vec<Npu> = Vec::new();
        for entry in glob("/sys/class/accel/accel?")?.flatten() {
            if let Ok(npu) = Self::from_sysfs_path(entry) {
                npu_vec.push(npu);
            }
        }

        debug!("{} NPUs found", npu_vec.len());

        Ok(npu_vec)
    }

    fn from_sysfs_path<P: AsRef<Path>>(path: P) -> Result<Npu> {
        let sysfs_device_path = path.as_ref().join("device");
        let mut uevent_contents: HashMap<String, String> = HashMap::new();
        let uevent_raw = std::fs::read_to_string(sysfs_device_path.join("uevent"))?;

        for line in uevent_raw.lines() {
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
                .context("error transforming PathBuf to str")?
        ))?
        .flatten()
        {
            hwmon_vec.push(hwmon);
        }

        let device = Device::from_vid_pid(vid, pid);

        let pci_slot = PciSlot::from_str(
            &uevent_contents
                .get("PCI_SLOT_NAME")
                .map_or_else(|| i18n("N/A"), std::string::ToString::to_string),
        )
        .context("can't turn PCI string to struct")?;

        let driver = uevent_contents
            .get("DRIVER")
            .map_or_else(|| i18n("N/A"), std::string::ToString::to_string);

        // if the driver is simple-framebuffer, it's likely not a GPU
        if driver == "simple-framebuffer" {
            bail!("this is a simple framebuffer");
        }

        let path = path.as_ref().to_path_buf();

        let (npu, npu_category) = if vid == VID_INTEL || driver == "intel_vpu" {
            (
                Npu::Intel(IntelNpu::new(
                    device,
                    pci_slot,
                    driver,
                    path,
                    hwmon_vec.first().cloned(),
                )),
                "Intel",
            )
        } else {
            (
                Npu::Other(OtherNpu::new(
                    device,
                    pci_slot,
                    driver,
                    path,
                    hwmon_vec.first().cloned(),
                )),
                "Other",
            )
        };

        info!(
            "Found NPU \"{}\" (PCI slot: {} · PCI ID: {vid:x}:{pid:x} · Category: {npu_category})",
            npu.name().unwrap_or("<unknown name>".into()),
            npu.pci_slot(),
        );

        Ok(npu)
    }

    pub fn get_vendor(&self) -> Result<&'static Vendor> {
        Ok(match self {
            Npu::Intel(npu) => npu.device(),
            Npu::Other(npu) => npu.device(),
        }
        .context("no device")?
        .vendor())
    }

    pub fn pci_slot(&self) -> PciSlot {
        match self {
            Npu::Intel(npu) => npu.pci_slot(),
            Npu::Other(npu) => npu.pci_slot(),
        }
    }

    pub fn driver(&self) -> String {
        match self {
            Npu::Intel(npu) => npu.driver(),
            Npu::Other(npu) => npu.driver(),
        }
    }

    pub fn name(&self) -> Result<String> {
        match self {
            Npu::Intel(npu) => npu.name(),
            Npu::Other(npu) => npu.name(),
        }
    }

    pub fn usage(&self) -> Result<f64> {
        match self {
            Npu::Intel(npu) => npu.usage(),
            Npu::Other(npu) => npu.usage(),
        }
    }

    pub fn used_vram(&self) -> Result<usize> {
        match self {
            Npu::Intel(npu) => npu.used_vram(),
            Npu::Other(npu) => npu.used_vram(),
        }
    }

    pub fn total_vram(&self) -> Result<usize> {
        match self {
            Npu::Intel(npu) => npu.total_vram(),
            Npu::Other(npu) => npu.total_vram(),
        }
    }

    pub fn temperature(&self) -> Result<f64> {
        match self {
            Npu::Intel(npu) => npu.temperature(),
            Npu::Other(npu) => npu.temperature(),
        }
    }

    pub fn power_usage(&self) -> Result<f64> {
        match self {
            Npu::Intel(npu) => npu.power_usage(),
            Npu::Other(npu) => npu.power_usage(),
        }
    }

    pub fn core_frequency(&self) -> Result<f64> {
        match self {
            Npu::Intel(npu) => npu.core_frequency(),
            Npu::Other(npu) => npu.core_frequency(),
        }
    }

    pub fn vram_frequency(&self) -> Result<f64> {
        match self {
            Npu::Intel(npu) => npu.vram_frequency(),
            Npu::Other(npu) => npu.vram_frequency(),
        }
    }

    pub fn power_cap(&self) -> Result<f64> {
        match self {
            Npu::Intel(npu) => npu.power_cap(),
            Npu::Other(npu) => npu.power_cap(),
        }
    }

    pub fn power_cap_max(&self) -> Result<f64> {
        match self {
            Npu::Intel(npu) => npu.power_cap_max(),
            Npu::Other(npu) => npu.power_cap_max(),
        }
    }
}
