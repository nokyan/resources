use anyhow::{Context, Result};
use gtk::gio::{Icon, ThemedIcon};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::i18n::{i18n, i18n_f};

use super::units::convert_storage;

const SYS_STATS: &str = r" *(?P<read_ios>[0-9]*) *(?P<read_merges>[0-9]*) *(?P<read_sectors>[0-9]*) *(?P<read_ticks>[0-9]*) *(?P<write_ios>[0-9]*) *(?P<write_merges>[0-9]*) *(?P<write_sectors>[0-9]*) *(?P<write_ticks>[0-9]*) *(?P<in_flight>[0-9]*) *(?P<io_ticks>[0-9]*) *(?P<time_in_queue>[0-9]*) *(?P<discard_ios>[0-9]*) *(?P<discard_merges>[0-9]*) *(?P<discard_sectors>[0-9]*) *(?P<discard_ticks>[0-9]*) *(?P<flush_ios>[0-9]*) *(?P<flush_ticks>[0-9]*)";

static RE_DRIVE: Lazy<Regex> = Lazy::new(|| Regex::new(SYS_STATS).unwrap());

#[derive(Debug)]
pub struct DriveData {
    pub inner: Drive,
    pub is_virtual: bool,
    pub writable: bool,
    pub removable: bool,
    pub disk_stats: HashMap<String, usize>,
    pub capacity: u64,
}

impl DriveData {
    pub async fn new(path: &Path) -> Self {
        let inner = Drive::from_sysfs(&path).await.unwrap_or_default();
        let is_virtual = inner.is_virtual().await;
        let writable = inner.writable().await.unwrap_or_default();
        let removable = inner.removable().await.unwrap_or_default();
        let disk_stats = inner.sys_stats().await.unwrap_or_default();
        let capacity = inner.capacity().await.unwrap_or_default();

        Self {
            inner,
            is_virtual,
            writable,
            removable,
            disk_stats,
            capacity,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum DriveType {
    CdDvdBluray,
    Emmc,
    Flash,
    Floppy,
    Hdd,
    LoopDevice,
    MappedDevice,
    Nvme,
    Raid,
    RamDisk,
    Ssd,
    ZfsVolume,
    Zram,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, Eq)]
pub struct Drive {
    pub model: Option<String>,
    pub drive_type: DriveType,
    pub block_device: String,
    pub sysfs_path: PathBuf,
}

impl PartialEq for Drive {
    fn eq(&self, other: &Self) -> bool {
        self.block_device == other.block_device
    }
}

impl Drive {
    /// Creates a `Drive` using a SysFS Path
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn from_sysfs<P: AsRef<Path>>(sysfs_path: P) -> Result<Drive> {
        let path = sysfs_path.as_ref().to_path_buf();
        let block_device = path
            .file_name()
            .expect("sysfs path ends with \"..\"?")
            .to_string_lossy()
            .to_string();

        let mut drive = Self::default();
        drive.sysfs_path = path;
        drive.block_device = block_device;
        drive.model = drive
            .model()
            .await
            .ok()
            .map(|model| model.trim().to_string());
        drive.drive_type = drive.drive_type().await.unwrap_or_default();
        Ok(drive)
    }

    /// Returns the SysFS Paths of possible drives
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn get_sysfs_paths() -> Result<Vec<PathBuf>> {
        let mut list = Vec::new();
        let mut entries = tokio::fs::read_dir("/sys/block").await?;
        while let Some(entry) = entries.next_entry().await? {
            let block_device = entry.file_name().to_string_lossy().to_string();
            if block_device.is_empty() {
                continue;
            }
            list.push(entry.path());
        }
        Ok(list)
    }

    pub fn display_name(&self, capacity: f64) -> String {
        let capacity_formatted = convert_storage(capacity, true);
        match self.drive_type {
            DriveType::CdDvdBluray => i18n("CD/DVD/Blu-ray Drive"),
            DriveType::Floppy => i18n("Floppy Drive"),
            DriveType::LoopDevice => i18n_f("{} Loop Device", &[&capacity_formatted]),
            DriveType::MappedDevice => i18n_f("{} Mapped Device", &[&capacity_formatted]),
            DriveType::Raid => i18n_f("{} RAID", &[&capacity_formatted]),
            DriveType::RamDisk => i18n_f("{} RAM Disk", &[&capacity_formatted]),
            DriveType::Zram => i18n_f("{} zram Device", &[&capacity_formatted]),
            DriveType::ZfsVolume => i18n_f("{} ZFS Volume", &[&capacity_formatted]),
            _ => i18n_f("{} Drive", &[&capacity_formatted]),
        }
    }

    /// Returns the current SysFS stats for the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn sys_stats(&self) -> Result<HashMap<String, usize>> {
        let stat = tokio::fs::read_to_string(self.sysfs_path.join("stat"))
            .await
            .with_context(|| format!("unable to read /sys/block/{}/stat", self.block_device))?;

        let captures = RE_DRIVE
            .captures(&stat)
            .with_context(|| format!("unable to parse /sys/block/{}/stat", self.block_device))?;

        Ok(RE_DRIVE
            .capture_names()
            .flatten()
            .filter_map(|named_capture| {
                Some((
                    named_capture.to_string(),
                    captures.name(named_capture)?.as_str().parse().ok()?,
                ))
            })
            .collect())
    }

    async fn drive_type(&self) -> Result<DriveType> {
        if self.block_device.starts_with("nvme") {
            Ok(DriveType::Nvme)
        } else if self.block_device.starts_with("mmc") {
            Ok(DriveType::Emmc)
        } else if self.block_device.starts_with("fd") {
            Ok(DriveType::Floppy)
        } else if self.block_device.starts_with("sr") {
            Ok(DriveType::CdDvdBluray)
        } else if self.block_device.starts_with("zram") {
            Ok(DriveType::Zram)
        } else if self.block_device.starts_with("md") {
            Ok(DriveType::Raid)
        } else if self.block_device.starts_with("loop") {
            Ok(DriveType::LoopDevice)
        } else if self.block_device.starts_with("dm") {
            Ok(DriveType::MappedDevice)
        } else if self.block_device.starts_with("ram") {
            Ok(DriveType::RamDisk)
        } else if self.block_device.starts_with("zd") {
            Ok(DriveType::ZfsVolume)
        } else if let Ok(rotational) =
            tokio::fs::read_to_string(self.sysfs_path.join("queue/rotational")).await
        {
            // turn rot into a boolean
            let rotational = rotational
                .replace('\n', "")
                .parse::<u8>()
                .map(|rot| rot != 0)?;
            if rotational {
                Ok(DriveType::Hdd)
            } else if self.removable().await? {
                Ok(DriveType::Flash)
            } else {
                Ok(DriveType::Ssd)
            }
        } else {
            Ok(DriveType::Unknown)
        }
    }

    /// Returns, whether the drive is removable
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn removable(&self) -> Result<bool> {
        tokio::fs::read_to_string(self.sysfs_path.join("removable"))
            .await?
            .replace('\n', "")
            .parse::<u8>()
            .map(|rem| rem != 0)
            .with_context(|| "unable to parse removable sysfs file")
    }

    /// Returns, whether the drive is writable
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn writable(&self) -> Result<bool> {
        tokio::fs::read_to_string(self.sysfs_path.join("ro"))
            .await?
            .replace('\n', "")
            .parse::<u8>()
            .map(|ro| ro == 0)
            .with_context(|| "unable to parse ro sysfs file")
    }

    /// Returns the capacity of the drive **in bytes**
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn capacity(&self) -> Result<u64> {
        tokio::fs::read_to_string(self.sysfs_path.join("size"))
            .await?
            .replace('\n', "")
            .parse::<u64>()
            .map(|sectors| sectors * 512)
            .with_context(|| "unable to parse size sysfs file")
    }

    /// Returns the model information of the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn model(&self) -> Result<String> {
        tokio::fs::read_to_string(self.sysfs_path.join("device/model"))
            .await
            .with_context(|| "unable to parse model sysfs file")
    }

    /// Returns the World-Wide Identification of the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn wwid(&self) -> Result<String> {
        tokio::fs::read_to_string(self.sysfs_path.join("device/wwid"))
            .await
            .with_context(|| "unable to parse wwid sysfs file")
    }

    /// Returns the appropriate Icon for the type of drive
    pub fn icon(&self) -> Icon {
        match self.drive_type {
            DriveType::CdDvdBluray => ThemedIcon::new("cd-dvd-bluray-symbolic").into(),
            DriveType::Emmc => ThemedIcon::new("emmc-symbolic").into(),
            DriveType::Flash => ThemedIcon::new("flash-symbolic").into(),
            DriveType::Floppy => ThemedIcon::new("floppy-symbolic").into(),
            DriveType::Hdd => ThemedIcon::new("hdd-symbolic").into(),
            DriveType::LoopDevice => ThemedIcon::new("loop-device-symbolic").into(),
            DriveType::MappedDevice => ThemedIcon::new("mapped-device-symbolic").into(),
            DriveType::Nvme => ThemedIcon::new("nvme-symbolic").into(),
            DriveType::Raid => ThemedIcon::new("raid-symbolic").into(),
            DriveType::RamDisk => ThemedIcon::new("ram-disk-symbolic").into(),
            DriveType::Ssd => ThemedIcon::new("ssd-symbolic").into(),
            DriveType::ZfsVolume => ThemedIcon::new("zfs-symbolic").into(),
            DriveType::Zram => ThemedIcon::new("zram-symbolic").into(),
            DriveType::Unknown => Self::default_icon(),
        }
    }

    pub async fn is_virtual(&self) -> bool {
        match self.drive_type {
            DriveType::LoopDevice => true,
            DriveType::MappedDevice => true,
            DriveType::Raid => true,
            DriveType::RamDisk => true,
            DriveType::ZfsVolume => true,
            DriveType::Zram => true,
            _ => self.capacity().await.unwrap_or(0) == 0,
        }
    }

    pub fn default_icon() -> Icon {
        ThemedIcon::new("unknown-drive-type-symbolic").into()
    }
}
