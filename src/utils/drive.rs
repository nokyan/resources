use anyhow::{Context, Result};
use async_std::stream::StreamExt;
use gtk::gio::{Icon, ThemedIcon};
use regex::Regex;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

static RE_DRIVE: OnceLock<Regex> = OnceLock::new();

const SYS_STATS: &str = r" *(?P<read_ios>[0-9]*) *(?P<read_merges>[0-9]*) *(?P<read_sectors>[0-9]*) *(?P<read_ticks>[0-9]*) *(?P<write_ios>[0-9]*) *(?P<write_merges>[0-9]*) *(?P<write_sectors>[0-9]*) *(?P<write_ticks>[0-9]*) *(?P<in_flight>[0-9]*) *(?P<io_ticks>[0-9]*) *(?P<time_in_queue>[0-9]*) *(?P<discard_ios>[0-9]*) *(?P<discard_merges>[0-9]*) *(?P<discard_sectors>[0-9]*) *(?P<discard_ticks>[0-9]*) *(?P<flush_ios>[0-9]*) *(?P<flush_ticks>[0-9]*)";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DriveType {
    CdDvdBluray,
    Emmc,
    Flash,
    Floppy,
    Hdd,
    Nvme,
    #[default]
    Unknown,
    Ssd,
}

#[derive(Debug, Clone, Default, Eq)]
pub struct Drive {
    pub model: Option<String>,
    pub drive_type: DriveType,
    pub block_device: String,
    pub sys_fs_path: PathBuf,
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
        drive.sys_fs_path = path;
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
    pub async fn get_sysfs_paths(skip_virtual_devices: bool) -> Result<Vec<PathBuf>> {
        let mut list = Vec::new();
        let mut entries = async_std::fs::read_dir("/sys/block").await?;
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let block_device = entry.file_name().to_string_lossy().to_string();
            if block_device.is_empty()
                || (skip_virtual_devices
                    && (block_device.starts_with("loop")
                        || block_device.starts_with("ram")
                        || block_device.starts_with("zram")
                        || block_device.starts_with("md")
                        || block_device.starts_with("dm")
                        || block_device.starts_with("zd")))
            {
                continue;
            }
            list.push(entry.path().into());
        }
        Ok(list)
    }

    /// Returns the current SysFS stats for the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn sys_stats(&self) -> Result<HashMap<String, usize>> {
        let stat = async_std::fs::read_to_string(self.sys_fs_path.join("stat"))
            .await
            .with_context(|| format!("unable to read /sys/block/{}/stat", self.block_device))?;

        let re_drive = RE_DRIVE.get_or_init(|| Regex::new(SYS_STATS).unwrap());

        let captures = re_drive
            .captures(&stat)
            .with_context(|| format!("unable to parse /sys/block/{}/stat", self.block_device))?;

        Ok(re_drive
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
        } else if let Ok(rot) =
            async_std::fs::read_to_string(self.sys_fs_path.join("queue/rotational")).await
        {
            // turn rot into a boolean
            let rot = rot.replace('\n', "").parse::<u8>().map(|rot| rot != 0)?;
            if rot {
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
        async_std::fs::read_to_string(self.sys_fs_path.join("removable"))
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
        async_std::fs::read_to_string(self.sys_fs_path.join("ro"))
            .await?
            .replace('\n', "")
            .parse::<u8>()
            .map(|ro| ro == 0)
            .with_context(|| "unable to parse ro sysfs file")
    }

    /// Returns the capacity **in sectors** of the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn capacity(&self) -> Result<u64> {
        async_std::fs::read_to_string(self.sys_fs_path.join("size"))
            .await?
            .replace('\n', "")
            .parse()
            .with_context(|| "unable to parse size sysfs file")
    }

    /// Returns the model information of the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn model(&self) -> Result<String> {
        async_std::fs::read_to_string(self.sys_fs_path.join("device/model"))
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
        async_std::fs::read_to_string(self.sys_fs_path.join("device/wwid"))
            .await
            .with_context(|| "unable to parse wwid sysfs file")
    }

    /// Returns the sector size of the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub async fn sector_size(&self) -> Result<u64> {
        async_std::fs::read_to_string(self.sys_fs_path.join("queue/hw_sector_size"))
            .await?
            .replace('\n', "")
            .parse()
            .with_context(|| "unable to parse hw_sector_size")
    }

    /// Returns the appropriate Icon for the type of drive
    pub fn icon(&self) -> Icon {
        match self.drive_type {
            DriveType::CdDvdBluray => ThemedIcon::new("cd-dvd-bluray-symbolic").into(),
            DriveType::Emmc => ThemedIcon::new("emmc-symbolic").into(),
            DriveType::Flash => ThemedIcon::new("flash-symbolic").into(),
            DriveType::Floppy => ThemedIcon::new("floppy-symbolic").into(),
            DriveType::Hdd => ThemedIcon::new("hdd-symbolic").into(),
            DriveType::Nvme => ThemedIcon::new("nvme-symbolic").into(),
            DriveType::Unknown => Self::default_icon(),
            DriveType::Ssd => ThemedIcon::new("ssd-symbolic").into(),
        }
    }

    pub fn default_icon() -> Icon {
        ThemedIcon::new("unknown-drive-type-symbolic").into()
    }
}
