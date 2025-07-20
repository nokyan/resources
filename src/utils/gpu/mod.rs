mod amd;
mod intel;
mod nvidia;
mod other;
mod v3d;

use anyhow::{Context, Result, bail};
use lazy_regex::{Lazy, Regex, lazy_regex};
use log::{debug, info, trace};
use process_data::{GpuIdentifier, pci_slot::PciSlot};
use v3d::V3dGpu;

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use self::{amd::AmdGpu, intel::IntelGpu, nvidia::NvidiaGpu, other::OtherGpu};
use crate::utils::{
    link::{Link, LinkData},
    read_sysfs,
};
use crate::{
    i18n::i18n,
    utils::{pci::Device, read_uevent},
};
use glob::glob;

use super::pci::Vendor;

pub const VID_AMD: u16 = 0x1002;
pub const VID_INTEL: u16 = 0x8086;
pub const VID_NVIDIA: u16 = 0x10DE;

static RE_CARD_ENUMARATOR: Lazy<Regex> = lazy_regex!(r"(\d+)\/?$");

#[derive(Debug)]
pub struct GpuData {
    pub gpu_identifier: GpuIdentifier,

    pub usage_fraction: Option<f64>,

    // in case of a GPU with a combined media engine, encode_fraction will contain the combined usage
    pub encode_fraction: Option<f64>,
    pub decode_fraction: Option<f64>,

    pub total_vram: Option<usize>,
    pub used_vram: Option<usize>,

    pub clock_speed: Option<f64>,
    pub vram_speed: Option<f64>,

    pub temperature: Option<f64>,

    pub power_usage: Option<f64>,
    pub power_cap: Option<f64>,
    pub power_cap_max: Option<f64>,

    pub link: Option<Link>,

    pub nvidia: bool,
}

impl GpuData {
    pub fn new(gpu: &Gpu) -> Self {
        let gpu_identifier = gpu.gpu_identifier();

        trace!("Gathering GPU data for {gpu_identifier}…");

        let usage_fraction = gpu.usage().map(|usage| usage.clamp(0.0, 1.0)).ok();

        let encode_fraction = gpu.encode_usage().map(|usage| usage.clamp(0.0, 1.0)).ok();

        let decode_fraction = gpu.decode_usage().map(|usage| usage.clamp(0.0, 1.0)).ok();

        let total_vram = gpu.total_vram().ok();
        let used_vram = gpu.used_vram().ok();

        let clock_speed = gpu.core_frequency().ok();
        let vram_speed = gpu.vram_frequency().ok();

        let temperature = gpu.temperature().ok();

        let power_usage = gpu.power_usage().ok();
        let power_cap = gpu.power_cap().ok();
        let power_cap_max = gpu.power_cap_max().ok();

        let link = gpu.link().ok();

        let nvidia = matches!(gpu, Gpu::Nvidia(_));

        let gpu_data = Self {
            gpu_identifier,
            usage_fraction,
            encode_fraction,
            decode_fraction,
            total_vram,
            used_vram,
            clock_speed,
            vram_speed,
            temperature,
            power_usage,
            power_cap,
            power_cap_max,
            link,
            nvidia,
        };

        trace!("Gathered GPU data for {gpu_identifier}: {gpu_data:?}");

        gpu_data
    }
}

#[derive(Debug, Clone)]
pub enum Gpu {
    Amd(AmdGpu),
    Intel(IntelGpu),
    Nvidia(NvidiaGpu),
    V3d(V3dGpu),
    Other(OtherGpu),
}

impl Default for Gpu {
    fn default() -> Self {
        Gpu::Other(OtherGpu::default())
    }
}

pub trait GpuImpl {
    fn device(&self) -> Option<&'static Device>;
    fn gpu_identifier(&self) -> GpuIdentifier;
    fn driver(&self) -> &str;
    fn sysfs_path(&self) -> &Path;
    fn first_hwmon(&self) -> Option<&Path>;

    fn name(&self) -> Result<String>;
    fn usage(&self) -> Result<f64>;
    fn encode_usage(&self) -> Result<f64>;
    fn decode_usage(&self) -> Result<f64>;
    fn combined_media_engine(&self) -> Result<bool>;
    fn used_vram(&self) -> Result<usize>;
    fn total_vram(&self) -> Result<usize>;
    fn temperature(&self) -> Result<f64>;
    fn power_usage(&self) -> Result<f64>;
    fn core_frequency(&self) -> Result<f64>;
    fn vram_frequency(&self) -> Result<f64>;
    fn power_cap(&self) -> Result<f64>;
    fn power_cap_max(&self) -> Result<f64>;

    // These are preimplemented ways of getting information through the DRM and hwmon interface.
    // It's also used as a fallback.

    fn drm_name(&self) -> Result<String> {
        Ok(self.device().context("no device")?.name().to_owned())
    }

    fn drm_usage(&self) -> Result<isize> {
        read_sysfs(self.sysfs_path().join("device/gpu_busy_percent"))
    }

    fn drm_used_vram(&self) -> Result<isize> {
        read_sysfs(self.sysfs_path().join("device/mem_info_vram_used"))
    }

    fn drm_total_vram(&self) -> Result<isize> {
        read_sysfs(self.sysfs_path().join("device/mem_info_vram_total"))
    }

    fn hwmon_path(&self) -> Result<&Path> {
        self.first_hwmon().context("no hwmon found")
    }

    fn hwmon_temperature(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("temp1_input")).map(|temp| temp as f64 / 1000.0)
    }

    fn hwmon_power_usage(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("power1_average"))
            .or_else(|_| read_sysfs::<isize>(self.hwmon_path()?.join("power1_input")))
            .map(|power| power as f64 / 1_000_000.0)
    }

    fn hwmon_core_frequency(&self) -> Result<f64> {
        read_sysfs::<isize>(self.hwmon_path()?.join("freq1_input")).map(|freq| freq as f64)
    }

    fn hwmon_vram_frequency(&self) -> Result<f64> {
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

impl std::ops::Deref for Gpu {
    type Target = dyn GpuImpl;

    fn deref(&self) -> &Self::Target {
        match self {
            Gpu::Amd(gpu) => gpu,
            Gpu::Intel(gpu) => gpu,
            Gpu::Nvidia(gpu) => gpu,
            Gpu::V3d(gpu) => gpu,
            Gpu::Other(gpu) => gpu,
        }
    }
}

impl Gpu {
    /// Returns a `Vec` of all GPUs currently found in the system.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems detecting
    /// the GPUs in the system
    pub fn get_gpus() -> Result<Vec<Gpu>> {
        debug!("Searching for GPUs…");

        let mut gpu_vec: Vec<Gpu> = Vec::new();
        for (i, entry) in glob("/sys/class/drm/card?")?.flatten().enumerate() {
            if let Ok(gpu) = Self::from_sysfs_path(entry, i) {
                gpu_vec.push(gpu);
            }
        }

        debug!("{} GPUs found", gpu_vec.len());

        Ok(gpu_vec)
    }

    fn from_sysfs_path<P: AsRef<Path>>(path: P, i: usize) -> Result<Gpu> {
        let path = path.as_ref().to_path_buf();

        trace!("Creating GPU object of {path:?}…");

        let enumarator = RE_CARD_ENUMARATOR
            .captures(&path.to_string_lossy())
            .and_then(|captures| captures.get(1))
            .and_then(|capture| capture.as_str().parse().ok())
            .unwrap_or(i);

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
            sysfs_device_path.to_string_lossy()
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
        .context("can't turn PCI string to struct");

        let gpu_identifier = if let Ok(pci_slot) = pci_slot {
            GpuIdentifier::PciSlot(pci_slot)
        } else {
            GpuIdentifier::Enumerator(enumarator)
        };

        let driver = uevent_contents
            .get("DRIVER")
            .map_or_else(|| i18n("N/A"), std::string::ToString::to_string);

        // if the driver is simple-framebuffer, it's likely not a GPU
        if driver == "simple-framebuffer" {
            bail!("this is a simple framebuffer");
        }

        let (gpu, gpu_category) = if vid == VID_AMD || driver == "amdgpu" {
            (
                Gpu::Amd(AmdGpu::new(
                    device,
                    gpu_identifier,
                    driver,
                    path.to_path_buf(),
                    hwmon_vec.first().cloned(),
                )),
                "AMD",
            )
        } else if vid == VID_INTEL || driver == "i915" {
            (
                Gpu::Intel(IntelGpu::new(
                    device,
                    gpu_identifier,
                    driver,
                    path.to_path_buf(),
                    hwmon_vec.first().cloned(),
                )),
                "Intel",
            )
        } else if vid == VID_NVIDIA || driver == "nvidia" {
            (
                Gpu::Nvidia(NvidiaGpu::new(
                    device,
                    gpu_identifier,
                    driver,
                    path.to_path_buf(),
                    hwmon_vec.first().cloned(),
                )),
                "NVIDIA",
            )
        } else if driver == "v3d" {
            (
                Gpu::V3d(V3dGpu::new(
                    device,
                    gpu_identifier,
                    driver,
                    path.to_path_buf(),
                    hwmon_vec.first().cloned(),
                )),
                "v3d",
            )
        } else {
            (
                Gpu::Other(OtherGpu::new(
                    device,
                    gpu_identifier,
                    driver,
                    path.to_path_buf(),
                    hwmon_vec.first().cloned(),
                )),
                "Other",
            )
        };

        info!(
            "Found GPU \"{}\" (Identifier: {} · PCI ID: {vid:x}:{pid:x} · Category: {gpu_category})",
            gpu.name().unwrap_or("<unknown name>".into()),
            gpu.gpu_identifier(),
        );

        trace!("Created GPU object of {path:?}: {gpu:?}");

        Ok(gpu)
    }

    pub fn get_vendor(&self) -> Result<&'static Vendor> {
        Ok(self.device().context("no device")?.vendor())
    }

    pub fn link(&self) -> Result<Link> {
        if let GpuIdentifier::PciSlot(pci_slot) = self.gpu_identifier() {
            let pcie_link = LinkData::from_pci_slot(&pci_slot)?;
            Ok(Link::Pcie(pcie_link))
        } else {
            bail!("Could not retrieve PciSlot from Gpu");
        }
    }
}
