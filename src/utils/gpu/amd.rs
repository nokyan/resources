use anyhow::{Context, Result, bail};
use lazy_regex::{Lazy, Regex, lazy_regex};
use log::{debug, trace, warn};
use process_data::GpuIdentifier;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Instant,
};

use crate::utils::{
    IS_FLATPAK,
    pci::{self, Device},
    read_sysfs,
};

use super::GpuImpl;

static RE_AMDGPU_IDS: Lazy<Regex> = lazy_regex!(r"([0-9A-F]{4}),\s*([0-9A-F]{2}),\s*(.*)");

static AMDGPU_IDS: LazyLock<HashMap<(u16, u8), String>> = LazyLock::new(|| {
    AmdGpu::read_libdrm_ids()
        .inspect_err(|e| warn!("Unable to parse amdgpu.ids!\n{e}\n{}", e.backtrace()))
        .unwrap_or_default()
});

#[derive(Debug, Clone, Default)]

pub struct AmdGpu {
    pub device: Option<&'static Device>,
    pub gpu_identifier: GpuIdentifier,
    pub driver: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
    combined_media_engine: bool,
}

impl AmdGpu {
    pub fn new(
        device: Option<&'static Device>,
        gpu_identifier: GpuIdentifier,
        driver: String,
        sysfs_path: PathBuf,
        first_hwmon_path: Option<PathBuf>,
    ) -> Self {
        let mut gpu = Self {
            device,
            gpu_identifier,
            driver,
            sysfs_path,
            first_hwmon_path,
            combined_media_engine: false,
        };

        if let Ok(vcn_version) = read_sysfs::<isize>(
            gpu.sysfs_path()
                .join("device/ip_discovery/die/0/UVD/0/major"),
        ) {
            if vcn_version >= 4 {
                gpu.combined_media_engine = true;
            }
        }

        gpu
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
                trace!("Found {name} ({device_id:04x}, rev {revision:02x})");
                map.insert((device_id, revision), name);
            }
        }

        let elapsed = start.elapsed();

        debug!(
            "Successfully parsed {} within {elapsed:.2?} ({} entries)",
            path.to_string_lossy(),
            map.len()
        );

        Ok(map)
    }
}

impl GpuImpl for AmdGpu {
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
        let revision = u8::from_str_radix(
            read_sysfs::<String>(self.sysfs_path().join("device/revision"))?
                .strip_prefix("0x")
                .context("missing hex prefix")?,
            16,
        )?;
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

    fn usage(&self) -> Result<f64> {
        self.drm_usage().map(|usage| usage as f64 / 100.0)
    }

    fn encode_usage(&self) -> Result<f64> {
        bail!("encode usage not implemented for AMD")
    }

    fn decode_usage(&self) -> Result<f64> {
        bail!("decode usage not implemented for AMD")
    }

    fn combined_media_engine(&self) -> Result<bool> {
        Ok(self.combined_media_engine)
    }

    fn used_vram(&self) -> Result<usize> {
        self.drm_used_vram().map(|usage| usage as usize)
    }

    fn total_vram(&self) -> Result<usize> {
        self.drm_total_vram().map(|usage| usage as usize)
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
