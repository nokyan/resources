use anyhow::{bail, Result};
use hashbrown::HashMap;
use log::{debug, warn};
use process_data::pci_slot::PciSlot;
use regex::Regex;

use std::{path::PathBuf, sync::LazyLock, time::Instant};

use crate::utils::{
    pci::{self, Device},
    IS_FLATPAK,
};

use super::GpuImpl;

static RE_AMDGPU_IDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9A-F]{4}),\s*([0-9A-F]{2}),\s*(.*)").unwrap());

static AMDGPU_IDS: LazyLock<HashMap<(u16, u8), String>> = LazyLock::new(|| {
    AmdGpu::read_libdrm_ids()
        .inspect_err(|e| warn!("Unable to parse amdgpu.ids! Stacktrace:\n{}", e.backtrace()))
        .unwrap_or_default()
});

#[derive(Debug, Clone, Default)]

pub struct AmdGpu {
    pub device: Option<&'static Device>,
    pub pci_slot: PciSlot,
    pub driver: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
}

impl AmdGpu {
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
            sysfs_path,
            first_hwmon_path,
        }
    }

    pub fn read_libdrm_ids() -> Result<HashMap<(u16, u8), String>> {
        let path = if *IS_FLATPAK {
            PathBuf::from("/run/host/usr/share/libdrm/amdgpu.ids")
        } else {
            PathBuf::from("/usr/share/libdrm/amdgpu.ids")
        };

        debug!("Parsing {}…", path.to_string_lossy());

        let start = Instant::now();

        let mut map = HashMap::new();

        let amdgpu_ids_raw = std::fs::read_to_string(&path)?;

        for capture in RE_AMDGPU_IDS.captures_iter(&amdgpu_ids_raw) {
            if let (Some(device_id), Some(revision), Some(name)) =
                (capture.get(1), capture.get(2), capture.get(3))
            {
                let device_id = u16::from_str_radix(device_id.as_str().trim(), 16).unwrap();
                let revision = u8::from_str_radix(revision.as_str().trim(), 16).unwrap();
                let name = name.as_str().into();
                map.insert((device_id, revision), name);
            }
        }

        let elapsed = start.elapsed();

        debug!(
            "Successfully parsed {} within {elapsed:.2?}",
            path.to_string_lossy()
        );

        Ok(map)
    }
}

impl GpuImpl for AmdGpu {
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
        let revision =
            u8::from_str_radix(&self.read_device_file("revision")?.replace("0x", ""), 16)?;
        Ok((*AMDGPU_IDS)
            .get(&(self.device().map_or(0, pci::Device::pid), revision))
            .cloned()
            .unwrap_or_else(|| {
                if let Ok(drm_name) = self.drm_name() {
                    format!("AMD Radeon Graphics ({drm_name})")
                } else {
                    "AMD Radeon Graphics".into()
                }
            }))
    }

    fn usage(&self) -> Result<isize> {
        self.drm_usage()
    }

    fn encode_usage(&self) -> Result<isize> {
        bail!("encode usage not implemented for AMD")
    }

    fn decode_usage(&self) -> Result<isize> {
        bail!("decode usage not implemented for AMD")
    }

    fn used_vram(&self) -> Result<isize> {
        self.drm_used_vram()
    }

    fn total_vram(&self) -> Result<isize> {
        self.drm_total_vram()
    }

    fn temperature(&self) -> Result<f64> {
        self.hwmon_temperature()
    }

    fn power_usage(&self) -> Result<f64> {
        self.hwmon_power_usage()
    }

    fn core_frequency(&self) -> Result<f64> {
        self.hwmon_core_frequency()
    }

    fn vram_frequency(&self) -> Result<f64> {
        self.hwmon_vram_frequency()
    }

    fn power_cap(&self) -> Result<f64> {
        self.hwmon_power_cap()
    }

    fn power_cap_max(&self) -> Result<f64> {
        self.hwmon_power_cap_max()
    }
}
