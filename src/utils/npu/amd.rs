use anyhow::{Context, Result, bail};
use process_data::pci_slot::PciSlot;

use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::utils::pci::Device;

use super::NpuImpl;

const DRM_IOCTL_BASE: u8 = b'd';
const DRM_IOCTL_VERSION: u8 = 0;
const DRM_COMMAND_BASE: u8 = 0x40;
const DRM_AMDXDNA_GET_INFO: u8 = 7;
const DRM_AMDXDNA_QUERY_CLOCK_METADATA: u32 = 3;
const DRM_AMDXDNA_QUERY_SENSORS: u32 = 4;
const DRM_AMDXDNA_QUERY_RESOURCE_INFO: u32 = 12;

const AMDXDNA_SENSOR_TYPE_POWER: u8 = 0;
const AMDXDNA_SENSOR_TYPE_COLUMN_UTILIZATION: u8 = 1;

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
struct DrmVersion {
    version_major: libc::c_int,
    version_minor: libc::c_int,
    version_patchlevel: libc::c_int,
    name_len: libc::size_t,
    name: *mut libc::c_char,
    date_len: libc::size_t,
    date: *mut libc::c_char,
    desc_len: libc::size_t,
    desc: *mut libc::c_char,
}

#[repr(C)]
#[derive(Default)]
struct AmdxdnaDrmGetResourceInfo {
    npu_clk_max: u64,
    npu_tops_max: u64,
    npu_task_max: u64,
    npu_tops_curr: u64,
    npu_task_curr: u64,
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

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AmdxdnaDrmQuerySensor {
    label: [u8; 64],
    input: u32,
    max: u32,
    average: u32,
    highest: u32,
    status: [u8; 64],
    units: [u8; 16],
    unitm: i8,
    sensor_type: u8,
    _pad: [u8; 6],
}

impl Default for AmdxdnaDrmQuerySensor {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
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

    fn drm_version(&self) -> Result<(i32, i32)> {
        let accel_name = self
            .sysfs_path
            .file_name()
            .context("invalid sysfs path")?
            .to_str()
            .context("invalid accel name")?;
        let dev_path = format!("/dev/accel/{accel_name}");

        let file = File::open(&dev_path).context("failed to open accel device")?;
        let fd = file.as_raw_fd();

        let mut version = DrmVersion {
            version_major: 0,
            version_minor: 0,
            version_patchlevel: 0,
            name_len: 0,
            name: std::ptr::null_mut(),
            date_len: 0,
            date: std::ptr::null_mut(),
            desc_len: 0,
            desc: std::ptr::null_mut(),
        };

        let ioctl_cmd = iowr::<DrmVersion>(DRM_IOCTL_BASE, DRM_IOCTL_VERSION);

        let ret = unsafe { libc::ioctl(fd, ioctl_cmd, &mut version) };
        if ret < 0 {
            bail!("ioctl failed: {}", std::io::Error::last_os_error());
        }

        Ok((version.version_major, version.version_minor))
    }

    fn has_drm_version(&self, required_major: i32, required_minor: i32) -> Result<bool> {
        let (major, minor) = self.drm_version()?;
        Ok(major > required_major || (major == required_major && minor >= required_minor))
    }

    fn query_resource_info(&self) -> Result<AmdxdnaDrmGetResourceInfo> {
        let accel_name = self
            .sysfs_path
            .file_name()
            .context("invalid sysfs path")?
            .to_str()
            .context("invalid accel name")?;
        let dev_path = format!("/dev/accel/{accel_name}");

        let file = File::open(&dev_path).context("failed to open accel device")?;
        let fd = file.as_raw_fd();

        let mut resource_info = AmdxdnaDrmGetResourceInfo::default();
        let mut get_info = AmdxdnaDrmGetInfo {
            param: DRM_AMDXDNA_QUERY_RESOURCE_INFO,
            buffer_size: std::mem::size_of::<AmdxdnaDrmGetResourceInfo>() as u32,
            buffer: &mut resource_info as *mut _ as u64,
        };

        let ioctl_cmd =
            iowr::<AmdxdnaDrmGetInfo>(DRM_IOCTL_BASE, DRM_COMMAND_BASE + DRM_AMDXDNA_GET_INFO);

        let ret = unsafe { libc::ioctl(fd, ioctl_cmd, &mut get_info) };
        if ret < 0 {
            bail!("ioctl failed: {}", std::io::Error::last_os_error());
        }

        Ok(resource_info)
    }

    fn query_sensors(&self) -> Result<Vec<AmdxdnaDrmQuerySensor>> {
        let accel_name = self
            .sysfs_path
            .file_name()
            .context("invalid sysfs path")?
            .to_str()
            .context("invalid accel name")?;
        let dev_path = format!("/dev/accel/{accel_name}");

        let file = File::open(&dev_path).context("failed to open accel device")?;
        let fd = file.as_raw_fd();

        let mut sensors = [AmdxdnaDrmQuerySensor::default(); 16];
        let mut get_info = AmdxdnaDrmGetInfo {
            param: DRM_AMDXDNA_QUERY_SENSORS,
            buffer_size: std::mem::size_of_val(&sensors) as u32,
            buffer: sensors.as_mut_ptr() as u64,
        };

        let ioctl_cmd =
            iowr::<AmdxdnaDrmGetInfo>(DRM_IOCTL_BASE, DRM_COMMAND_BASE + DRM_AMDXDNA_GET_INFO);

        let ret = unsafe { libc::ioctl(fd, ioctl_cmd, &mut get_info) };
        if ret < 0 {
            bail!("ioctl failed: {}", std::io::Error::last_os_error());
        }

        let actual_sensors =
            get_info.buffer_size as usize / std::mem::size_of::<AmdxdnaDrmQuerySensor>();
        Ok(sensors[..actual_sensors].to_vec())
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
        if !self.has_drm_version(0, 7)? {
            bail!("usage not implemented by kernel");
        }

        let sensors = self.query_sensors()?;
        let mut usage_sum = 0.0;
        let mut usage_count = 0;
        for s in &sensors {
            if s.sensor_type == AMDXDNA_SENSOR_TYPE_COLUMN_UTILIZATION {
                let val = s.input as f64 * 10.0f64.powi(s.unitm as i32);
                usage_sum += val;
                usage_count += 1;
            }
        }
        if usage_count > 0 {
            // Return average utilization as a fraction [0, 1]
            // Each column's 'input' is in range [0, 100] as per driver source
            return Ok(usage_sum / (usage_count as f64 * 100.0));
        }

        bail!("usage not implemented by kernel")
    }

    fn used_memory(&self) -> Result<u64> {
        self.drm_used_memory()
    }

    fn total_memory(&self) -> Result<u64> {
        self.drm_total_memory()
    }

    fn temperature(&self) -> Result<f64> {
        self.hwmon_temperature()
    }

    fn power_usage(&self) -> Result<f64> {
        if let Ok(sensors) = self.query_sensors() {
            for s in &sensors {
                if s.sensor_type == AMDXDNA_SENSOR_TYPE_POWER {
                    let val = s.input as f64 * 10.0f64.powi(s.unitm as i32);
                    return Ok(val);
                }
            }
        }
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

    fn tops(&self) -> Result<u64> {
        let resource_info = self.query_resource_info()?;
        Ok(resource_info.npu_tops_curr)
    }

    fn max_tops(&self) -> Result<u64> {
        let resource_info = self.query_resource_info()?;
        Ok(resource_info.npu_tops_max)
    }

    fn power_cap(&self) -> Result<f64> {
        self.hwmon_power_cap()
    }

    fn power_cap_max(&self) -> Result<f64> {
        self.hwmon_power_cap_max()
    }
}
