use anyhow::{Context, Result};
use process_data::pci_slot::PciSlot;

use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::utils::pci::Device;

use super::NpuImpl;

const DRM_IOCTL_BASE: u8 = b'd';
const DRM_COMMAND_BASE: u8 = 0x40;
const DRM_AMDXDNA_GET_INFO: u8 = 7;
const DRM_AMDXDNA_QUERY_CLOCK_METADATA: u32 = 3;

const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;

const fn ioc(dir: u32, ty: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((dir << 30) | ((ty as u32) << 8) | (nr as u32) | ((size as u32) << 16)) as libc::c_ulong
}

const fn iowr<T>(ty: u8, nr: u8) -> libc::c_ulong {
    ioc(IOC_READ | IOC_WRITE, ty, nr, std::mem::size_of::<T>())
}

#[repr(C)]
struct AmdxdnaDrmGetInfo {
    param: u32,
    buffer_size: u32,
    buffer: u64,
}

#[repr(C)]
#[derive(Default)]
struct AmdxdnaDrmQueryClock {
    name: [u8; 16],
    freq_mhz: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Default)]
struct AmdxdnaDrmQueryClockMetadata {
    mp_npu_clock: AmdxdnaDrmQueryClock,
    h_clock: AmdxdnaDrmQueryClock,
}

#[derive(Debug, Clone, Default)]

pub struct AmdNpu {
    pub device: Option<&'static Device>,
    pub pci_slot: PciSlot,
    pub driver: String,
    sysfs_path: PathBuf,
    first_hwmon_path: Option<PathBuf>,
}

impl AmdNpu {
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

    fn query_clock_metadata(&self) -> Result<(u64, u64)> {
        let accel_name = self
            .sysfs_path
            .file_name()
            .context("invalid sysfs path")?
            .to_str()
            .context("invalid accel name")?;
        let dev_path = format!("/dev/accel/{accel_name}");

        let file = File::open(&dev_path).context("failed to open accel device")?;
        let fd = file.as_raw_fd();

        let mut clock_metadata = AmdxdnaDrmQueryClockMetadata::default();
        let mut get_info = AmdxdnaDrmGetInfo {
            param: DRM_AMDXDNA_QUERY_CLOCK_METADATA,
            buffer_size: std::mem::size_of::<AmdxdnaDrmQueryClockMetadata>() as u32,
            buffer: &mut clock_metadata as *mut _ as u64,
        };

        let ioctl_cmd =
            iowr::<AmdxdnaDrmGetInfo>(DRM_IOCTL_BASE, DRM_COMMAND_BASE + DRM_AMDXDNA_GET_INFO);

        let ret = unsafe { libc::ioctl(fd, ioctl_cmd, &mut get_info) };
        if ret < 0 {
            anyhow::bail!("ioctl failed: {}", std::io::Error::last_os_error());
        }

        let h_clock_hz = clock_metadata.h_clock.freq_mhz as u64 * 1_000_000;
        let mp_npu_clock_hz = clock_metadata.mp_npu_clock.freq_mhz as u64 * 1_000_000;

        Ok((h_clock_hz, mp_npu_clock_hz))
    }
}

impl NpuImpl for AmdNpu {
    fn device(&self) -> Option<&'static Device> {
        self.device
    }

    fn pci_slot(&self) -> PciSlot {
        self.pci_slot
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
        self.drm_name()
    }

    fn usage(&self) -> Result<f64> {
        self.drm_usage().map(|usage| usage as f64 / 100.0)
    }

    fn used_memory(&self) -> Result<usize> {
        self.drm_used_memory().map(|usage| usage as usize)
    }

    fn total_memory(&self) -> Result<usize> {
        self.drm_total_memory().map(|usage| usage as usize)
    }

    fn temperature(&self) -> Result<f64> {
        self.hwmon_temperature()
    }

    fn power_usage(&self) -> Result<f64> {
        self.hwmon_power_usage()
    }

    fn core_frequency(&self) -> Result<f64> {
        self.query_clock_metadata()
            .map(|(h_clock_hz, _)| h_clock_hz as f64)
    }

    fn memory_frequency(&self) -> Result<f64> {
        self.query_clock_metadata()
            .map(|(_, mp_npu_clock_hz)| mp_npu_clock_hz as f64)
    }

    fn power_cap(&self) -> Result<f64> {
        self.hwmon_power_cap()
    }

    fn power_cap_max(&self) -> Result<f64> {
        self.hwmon_power_cap_max()
    }
}
