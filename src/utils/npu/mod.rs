mod intel;
mod other;

use anyhow::{Context, Result, bail};
use log::{debug, info, trace};
use process_data::pci_slot::PciSlot;

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use glob::glob;

use crate::{
    i18n::i18n,
    utils::{pci::Device, read_sysfs, read_uevent},
};

use self::{intel::IntelNpu, other::OtherNpu};

use super::{
    link::{Link, LinkData},
    pci::Vendor,
};

pub const VID_INTEL: u16 = 0x8086;

#[derive(Debug)]
pub struct NpuData {
    pub pci_slot: PciSlot,

    pub usage_fraction: Option<f64>,

    pub total_memory: Option<usize>,
    pub used_memory: Option<usize>,

    pub clock_speed: Option<f64>,
    pub vram_speed: Option<f64>,

    pub temperature: Option<f64>,

    pub power_usage: Option<f64>,
    pub power_cap: Option<f64>,
    pub power_cap_max: Option<f64>,

    pub link: Option<Link>,
}

impl NpuData {
    pub fn new(npu: &Npu) -> Self {
        let pci_slot = npu.pci_slot();

        trace!("Gathering NPU data for {}…", pci_slot);

        let usage_fraction = npu.usage().ok();

        let total_memory = npu.total_vram().ok();
        let used_memory = npu.used_vram().ok();

        let clock_speed = npu.core_frequency().ok();
        let vram_speed = npu.memory_frequency().ok();

        let temperature = npu.temperature().ok();

        let power_usage = npu.power_usage().ok();
        let power_cap = npu.power_cap().ok();
        let power_cap_max = npu.power_cap_max().ok();

        let link = npu.link().ok();

        let npu_data = Self {
            pci_slot,
            usage_fraction,
            total_memory,
            used_memory,
            clock_speed,
            vram_speed,
            temperature,
            power_usage,
            power_cap,
            power_cap_max,
            link,
        };

        trace!("Gathered NPU data for {}: {npu_data:?}", pci_slot);

        npu_data
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
    fn driver(&self) -> &str;
    fn sysfs_path(&self) -> &Path;
    fn first_hwmon(&self) -> Option<&Path>;

    fn name(&self) -> Result<String>;
    fn usage(&self) -> Result<f64>;
    fn used_vram(&self) -> Result<usize>;
    fn total_vram(&self) -> Result<usize>;
    fn temperature(&self) -> Result<f64>;
    fn power_usage(&self) -> Result<f64>;
    fn core_frequency(&self) -> Result<f64>;
    fn memory_frequency(&self) -> Result<f64>;
    fn power_cap(&self) -> Result<f64>;
    fn power_cap_max(&self) -> Result<f64>;

    // These are preimplemented ways of getting information through the DRM and hwmon interface.
    // It's also used as a fallback.

    fn drm_name(&self) -> Result<String> {
        Ok(self.device().context("no device")?.name().to_owned())
    }

    fn drm_usage(&self) -> Result<isize> {
        // No NPU driver actually implements this yet, this is a guess for the future based on drm_usage() for GPUs
        read_sysfs(self.sysfs_path().join("device/npu_busy_percent"))
    }

    fn drm_used_memory(&self) -> Result<isize> {
        // ivpu will implement this with kernel 6.14, using this as a fallback just in case other vendors start using
        // this name as well
        read_sysfs(self.sysfs_path().join("device/npu_memory_utilization"))
    }

    fn drm_total_memory(&self) -> Result<isize> {
        // No NPU driver actually implements this yet, this is a guess for the future based on ivpu's
        // npu_memory_utilization
        read_sysfs(self.sysfs_path().join("device/npu_memory_total"))
    }

    fn hwmon_path(&self) -> Result<&Path> {
        self.first_hwmon().context("no hwmon found")
    }

    fn hwmon_temperature(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("temp1_input")).map(|temp| temp as f64 / 1000.0)
    }

    fn hwmon_power_usage(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("power1_average"))
            .map(|power| power as f64 / 1_000_000.0)
    }

    fn hwmon_core_frequency(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("freq1_input")).map(|freq| freq as f64)
    }

    fn hwmon_memory_frequency(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("freq2_input")).map(|freq| freq as f64)
    }

    fn hwmon_power_cap(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("power1_cap"))
            .map(|power| power as f64 / 1_000_000.0)
    }

    fn hwmon_power_cap_max(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("power1_cap_max"))
            .map(|power| power as f64 / 1_000_000.0)
    }
}

impl std::ops::Deref for Npu {
    type Target = dyn NpuImpl;

    fn deref(&self) -> &Self::Target {
        match self {
            Npu::Intel(npu) => npu,
            Npu::Other(npu) => npu,
        }
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
        let path = path.as_ref();

        trace!("Creating NPU object of {path:?}…");

        let sysfs_device_path = path.join("device");
        let uevent_contents = read_uevent(sysfs_device_path.join("uevent"))?;

        let (device, vid, pid) = if let Some(pci_line) = uevent_contents.get("PCI_ID") {
            let (vid_str, pid_str) = pci_line.split_once(':').unwrap_or(("0", "0"));
            let vid = u16::from_str_radix(vid_str, 16).unwrap_or_default();
            let pid = u16::from_str_radix(pid_str, 16).unwrap_or_default();
            (Device::from_vid_pid(vid, pid), vid, pid)
        } else {
            (None, 0, 0)
        };

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

        let (npu, npu_category) = if vid == VID_INTEL || driver == "intel_vpu" {
            (
                Npu::Intel(IntelNpu::new(
                    device,
                    pci_slot,
                    driver,
                    path.to_path_buf(),
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
                    path.to_path_buf(),
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

        trace!("Created NPU object of {path:?}: {npu:?}");

        Ok(npu)
    }

    pub fn get_vendor(&self) -> Result<&'static Vendor> {
        Ok(self.device().context("no device")?.vendor())
    }

    pub fn link(&self) -> Result<Link> {
        Ok(Link::Pcie(LinkData::from_pci_slot(&self.pci_slot())?))
    }
}
