mod amd;
mod intel;
mod nvidia;
mod other;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use glob::glob;
use pci_ids::Device;

use crate::i18n::i18n;

use self::{amd::AmdGpu, intel::IntelGpu, nvidia::NvidiaGpu, other::OtherGpu};

const VID_AMD: u16 = 4098;
const VID_INTEL: u16 = 32902;
const VID_NVIDIA: u16 = 4318;

#[derive(Debug)]
pub struct GpuData {
    pub usage_fraction: Option<f64>,

    pub encode_fraction: Option<f64>,
    pub decode_fraction: Option<f64>,

    pub total_vram: Option<isize>,
    pub used_vram: Option<isize>,

    pub clock_speed: Option<f64>,
    pub vram_speed: Option<f64>,

    pub temp: Option<f64>,

    pub power_usage: Option<f64>,
    pub power_cap: Option<f64>,
    pub power_cap_max: Option<f64>,
}

impl GpuData {
    pub async fn new(gpu: &Gpu) -> Self {
        let usage_fraction = gpu.usage().await.map(|usage| (usage as f64) / 100.0).ok();

        let encode_fraction = gpu
            .encode_usage()
            .await
            .map(|usage| (usage as f64) / 100.0)
            .ok();

        let decode_fraction = gpu
            .decode_usage()
            .await
            .map(|usage| (usage as f64) / 100.0)
            .ok();

        let total_vram = gpu.total_vram().await.ok();
        let used_vram = gpu.used_vram().await.ok();

        let clock_speed = gpu.core_frequency().await.ok();
        let vram_speed = gpu.vram_frequency().await.ok();

        let temp = gpu.temperature().await.ok();

        let power_usage = gpu.power_usage().await.ok();
        let power_cap = gpu.power_cap().await.ok();
        let power_cap_max = gpu.power_cap_max().await.ok();

        Self {
            usage_fraction,
            encode_fraction,
            decode_fraction,
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
pub enum Gpu {
    Amd(AmdGpu),
    Nvidia(NvidiaGpu),
    Intel(IntelGpu),
    Other(OtherGpu),
}

impl Default for Gpu {
    fn default() -> Self {
        Gpu::Other(OtherGpu::default())
    }
}

#[async_trait]
pub trait GpuImpl {
    fn device(&self) -> Option<&'static Device>;
    fn pci_slot(&self) -> String;
    fn driver(&self) -> String;
    fn sysfs_path(&self) -> PathBuf;
    fn first_hwmon(&self) -> Option<PathBuf>;

    fn name(&self) -> Result<String>;
    async fn usage(&self) -> Result<isize>;
    async fn encode_usage(&self) -> Result<isize>;
    async fn decode_usage(&self) -> Result<isize>;
    async fn used_vram(&self) -> Result<isize>;
    async fn total_vram(&self) -> Result<isize>;
    async fn temperature(&self) -> Result<f64>;
    async fn power_usage(&self) -> Result<f64>;
    async fn core_frequency(&self) -> Result<f64>;
    async fn vram_frequency(&self) -> Result<f64>;
    async fn power_cap(&self) -> Result<f64>;
    async fn power_cap_max(&self) -> Result<f64>;

    async fn read_sysfs_int<P: AsRef<Path> + std::marker::Send>(&self, file: P) -> Result<isize> {
        let path = self.sysfs_path().join(file);
        tokio::fs::read_to_string(&path)
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

    async fn read_device_int<P: AsRef<Path> + std::marker::Send>(&self, file: P) -> Result<isize> {
        let path = self.sysfs_path().join("device").join(file);
        tokio::fs::read_to_string(&path)
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

    async fn read_hwmon_int<P: AsRef<Path> + std::marker::Send>(&self, file: P) -> Result<isize> {
        let path = self.first_hwmon().context("no hwmon found")?.join(file);
        tokio::fs::read_to_string(&path)
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

    // These are preimplemented ways of getting information through the DRM and hwmon interface.
    // It's also used as a fallback.

    fn drm_name(&self) -> Result<String> {
        Ok(self.device().context("no device")?.name().to_owned())
    }

    async fn drm_usage(&self) -> Result<isize> {
        self.read_device_int("gpu_busy_percent").await
    }

    async fn drm_used_vram(&self) -> Result<isize> {
        self.read_device_int("mem_info_vram_used").await
    }

    async fn drm_total_vram(&self) -> Result<isize> {
        self.read_device_int("mem_info_vram_total").await
    }

    async fn hwmon_temperature(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("temp1_input").await? as f64 / 1000.0)
    }

    async fn hwmon_power_usage(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("power1_average").await? as f64 / 1_000_000.0)
    }

    async fn hwmon_core_frequency(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("freq1_input").await? as f64)
    }

    async fn hwmon_vram_frequency(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("freq2_input").await? as f64)
    }

    async fn hwmon_power_cap(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("power1_cap").await? as f64 / 1_000_000.0)
    }

    async fn hwmon_power_cap_max(&self) -> Result<f64> {
        Ok(self.read_hwmon_int("power1_cap_max").await? as f64 / 1_000_000.0)
    }
}

impl Gpu {
    /// Returns a `Vec` of all GPUs currently found in the system.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are problems detecting
    /// the GPUs in the system
    pub async fn get_gpus() -> Result<Vec<Gpu>> {
        let mut gpu_vec: Vec<Gpu> = Vec::new();
        for entry in glob("/sys/class/drm/card?")?.flatten() {
            let sysfs_device_path = entry.join("device");
            let mut uevent_contents: HashMap<String, String> = HashMap::new();
            let uevent_raw = tokio::fs::read_to_string(sysfs_device_path.join("uevent")).await?;

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

            let device = Device::from_vid_pid(vid, pid);

            let pci_slot = uevent_contents
                .get("PCI_SLOT_NAME")
                .map_or_else(|| i18n("N/A"), std::string::ToString::to_string);

            let driver = uevent_contents
                .get("DRIVER")
                .map_or_else(|| i18n("N/A"), std::string::ToString::to_string);

            let gpu = match vid {
                VID_AMD => Gpu::Amd(AmdGpu::new(
                    device,
                    pci_slot,
                    driver,
                    entry,
                    hwmon_vec.get(0).cloned(),
                )),
                VID_INTEL => Gpu::Intel(IntelGpu::new(
                    device,
                    pci_slot,
                    driver,
                    entry,
                    hwmon_vec.get(0).cloned(),
                )),
                VID_NVIDIA => Gpu::Nvidia(NvidiaGpu::new(
                    device,
                    pci_slot,
                    driver,
                    entry,
                    hwmon_vec.get(0).cloned(),
                )),
                _ => Gpu::Other(OtherGpu::new(
                    device,
                    pci_slot,
                    driver,
                    entry,
                    hwmon_vec.get(0).cloned(),
                )),
            };

            gpu_vec.push(gpu);
        }
        Ok(gpu_vec)
    }

    pub fn get_vendor(&self) -> Result<String> {
        Ok(match self {
            Gpu::Amd(gpu) => gpu.device(),
            Gpu::Nvidia(gpu) => gpu.device(),
            Gpu::Intel(gpu) => gpu.device(),
            Gpu::Other(gpu) => gpu.device(),
        }
        .context("no device")?
        .vendor()
        .name()
        .to_owned())
    }

    pub fn pci_slot(&self) -> String {
        match self {
            Gpu::Amd(gpu) => gpu.pci_slot(),
            Gpu::Nvidia(gpu) => gpu.pci_slot(),
            Gpu::Intel(gpu) => gpu.pci_slot(),
            Gpu::Other(gpu) => gpu.pci_slot(),
        }
    }

    pub fn driver(&self) -> String {
        match self {
            Gpu::Amd(gpu) => gpu.driver(),
            Gpu::Nvidia(gpu) => gpu.driver(),
            Gpu::Intel(gpu) => gpu.driver(),
            Gpu::Other(gpu) => gpu.driver(),
        }
    }

    pub fn name(&self) -> Result<String> {
        match self {
            Gpu::Amd(gpu) => gpu.name(),
            Gpu::Nvidia(gpu) => gpu.name(),
            Gpu::Intel(gpu) => gpu.name(),
            Gpu::Other(gpu) => gpu.name(),
        }
    }

    pub async fn usage(&self) -> Result<isize> {
        match self {
            Gpu::Amd(gpu) => gpu.usage(),
            Gpu::Nvidia(gpu) => gpu.usage(),
            Gpu::Intel(gpu) => gpu.usage(),
            Gpu::Other(gpu) => gpu.usage(),
        }
        .await
    }

    pub async fn encode_usage(&self) -> Result<isize> {
        match self {
            Gpu::Amd(gpu) => gpu.encode_usage(),
            Gpu::Nvidia(gpu) => gpu.encode_usage(),
            Gpu::Intel(gpu) => gpu.encode_usage(),
            Gpu::Other(gpu) => gpu.encode_usage(),
        }
        .await
    }

    pub async fn decode_usage(&self) -> Result<isize> {
        match self {
            Gpu::Amd(gpu) => gpu.decode_usage(),
            Gpu::Nvidia(gpu) => gpu.decode_usage(),
            Gpu::Intel(gpu) => gpu.decode_usage(),
            Gpu::Other(gpu) => gpu.decode_usage(),
        }
        .await
    }

    pub async fn used_vram(&self) -> Result<isize> {
        match self {
            Gpu::Amd(gpu) => gpu.used_vram(),
            Gpu::Nvidia(gpu) => gpu.used_vram(),
            Gpu::Intel(gpu) => gpu.used_vram(),
            Gpu::Other(gpu) => gpu.used_vram(),
        }
        .await
    }

    pub async fn total_vram(&self) -> Result<isize> {
        match self {
            Gpu::Amd(gpu) => gpu.total_vram(),
            Gpu::Nvidia(gpu) => gpu.total_vram(),
            Gpu::Intel(gpu) => gpu.total_vram(),
            Gpu::Other(gpu) => gpu.total_vram(),
        }
        .await
    }

    pub async fn temperature(&self) -> Result<f64> {
        match self {
            Gpu::Amd(gpu) => gpu.temperature(),
            Gpu::Nvidia(gpu) => gpu.temperature(),
            Gpu::Intel(gpu) => gpu.temperature(),
            Gpu::Other(gpu) => gpu.temperature(),
        }
        .await
    }

    pub async fn power_usage(&self) -> Result<f64> {
        match self {
            Gpu::Amd(gpu) => gpu.power_usage(),
            Gpu::Nvidia(gpu) => gpu.power_usage(),
            Gpu::Intel(gpu) => gpu.power_usage(),
            Gpu::Other(gpu) => gpu.power_usage(),
        }
        .await
    }

    pub async fn core_frequency(&self) -> Result<f64> {
        match self {
            Gpu::Amd(gpu) => gpu.core_frequency(),
            Gpu::Nvidia(gpu) => gpu.core_frequency(),
            Gpu::Intel(gpu) => gpu.core_frequency(),
            Gpu::Other(gpu) => gpu.core_frequency(),
        }
        .await
    }

    pub async fn vram_frequency(&self) -> Result<f64> {
        match self {
            Gpu::Amd(gpu) => gpu.vram_frequency(),
            Gpu::Nvidia(gpu) => gpu.vram_frequency(),
            Gpu::Intel(gpu) => gpu.vram_frequency(),
            Gpu::Other(gpu) => gpu.vram_frequency(),
        }
        .await
    }

    pub async fn power_cap(&self) -> Result<f64> {
        match self {
            Gpu::Amd(gpu) => gpu.power_cap(),
            Gpu::Nvidia(gpu) => gpu.power_cap(),
            Gpu::Intel(gpu) => gpu.power_cap(),
            Gpu::Other(gpu) => gpu.power_cap(),
        }
        .await
    }

    pub async fn power_cap_max(&self) -> Result<f64> {
        match self {
            Gpu::Amd(gpu) => gpu.power_cap_max(),
            Gpu::Nvidia(gpu) => gpu.power_cap_max(),
            Gpu::Intel(gpu) => gpu.power_cap_max(),
            Gpu::Other(gpu) => gpu.power_cap_max(),
        }
        .await
    }
}
