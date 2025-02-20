use super::units::convert_storage;
use crate::i18n::{i18n, i18n_f};
use crate::utils::link::{Link, LinkData};
use anyhow::{bail, Context, Result};
use gtk::gio::{Icon, ThemedIcon};
use lazy_regex::{lazy_regex, Lazy, Regex};
use log::trace;
use path_dedot::ParseDot;
use process_data::pci_slot::PciSlot;
use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    str::FromStr,
};

const PATH_SYSFS: &str = "/sys/block";

static RE_DRIVE: Lazy<Regex> = lazy_regex!(
    r" *(?P<read_ios>[0-9]*) *(?P<read_merges>[0-9]*) *(?P<read_sectors>[0-9]*) *(?P<read_ticks>[0-9]*) *(?P<write_ios>[0-9]*) *(?P<write_merges>[0-9]*) *(?P<write_sectors>[0-9]*) *(?P<write_ticks>[0-9]*) *(?P<in_flight>[0-9]*) *(?P<io_ticks>[0-9]*) *(?P<time_in_queue>[0-9]*) *(?P<discard_ios>[0-9]*) *(?P<discard_merges>[0-9]*) *(?P<discard_sectors>[0-9]*) *(?P<discard_ticks>[0-9]*) *(?P<flush_ios>[0-9]*) *(?P<flush_ticks>[0-9]*)"
);

static RE_ATA_LINK: Lazy<Regex> = lazy_regex!(r"(^link(\d+))$");

static RE_ATA_SLOT: Lazy<Regex> = lazy_regex!(r"(^.+?/ata(\d+))/");

#[derive(Debug)]
pub struct DriveData {
    pub inner: Drive,
    pub is_virtual: bool,
    pub writable: Result<bool>,
    pub removable: Result<bool>,
    pub disk_stats: HashMap<String, usize>,
    pub capacity: Result<u64>,
    pub link: Result<Link>,
}

impl DriveData {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();

        trace!("Gathering drive data for {path:?}…");

        let inner = Drive::from_sysfs(path);
        let is_virtual = inner.is_virtual();
        let writable = inner.writable();
        let removable = inner.removable();
        let disk_stats = inner.sys_stats().unwrap_or_default();
        let capacity = inner.capacity();
        let link = inner.link();

        let drive_data = Self {
            inner,
            is_virtual,
            writable,
            removable,
            disk_stats,
            capacity,
            link,
        };

        trace!(
            "Gathered drive data for {}: {drive_data:?}",
            path.to_string_lossy()
        );

        drive_data
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum DriveSlot {
    Pci(PciSlot),
    Ata(AtaSlot),
    #[default]
    Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct AtaSlot {
    pub ata_device: u8,
    pub ata_link: u8,
}

#[derive(Debug, Clone, Default, Eq)]
pub struct Drive {
    pub model: Option<String>,
    pub drive_type: DriveType,
    pub block_device: String,
    pub slot: DriveSlot,
    pub sysfs_path: PathBuf,
}

impl Display for DriveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DriveType::CdDvdBluray => i18n("CD/DVD/Blu-ray Drive"),
                DriveType::Emmc => i18n("eMMC Storage"),
                DriveType::Flash => i18n("Flash Storage"),
                DriveType::Floppy => i18n("Floppy Drive"),
                DriveType::Hdd => i18n("Hard Disk Drive"),
                DriveType::LoopDevice => i18n("Loop Device"),
                DriveType::MappedDevice => i18n("Mapped Device"),
                DriveType::Nvme => i18n("NVMe Drive"),
                DriveType::Unknown => i18n("N/A"),
                DriveType::Raid => i18n("Software Raid"),
                DriveType::RamDisk => i18n("RAM Disk"),
                DriveType::Ssd => i18n("Solid State Drive"),
                DriveType::ZfsVolume => i18n("ZFS Volume"),
                DriveType::Zram => i18n("Compressed RAM Disk (zram)"),
            }
        )
    }
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
    pub fn from_sysfs<P: AsRef<Path>>(sysfs_path: P) -> Drive {
        let path = sysfs_path.as_ref().to_path_buf();

        trace!("Creating Drive object of {path:?}");

        let block_device = path
            .file_name()
            .expect("sysfs path ends with \"..\"?")
            .to_string_lossy()
            .to_string();

        let mut drive = Self::default();
        drive.sysfs_path = path.clone();
        drive.block_device = block_device;
        drive.model = drive.model().ok().map(|model| model.trim().to_string());
        drive.drive_type = drive.drive_type().unwrap_or_default();
        drive.slot = drive.slot().unwrap_or_default();
        trace!("Created Drive object of {path:?}: {drive:?}");

        drive
    }

    /// Returns the SysFS Paths of possible drives
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub fn get_sysfs_paths() -> Result<Vec<PathBuf>> {
        let mut list = Vec::new();
        trace!("Finding entries in {PATH_SYSFS}");
        let entries = std::fs::read_dir(PATH_SYSFS)?;
        for entry in entries {
            let entry = entry?;
            let block_device = entry.file_name().to_string_lossy().to_string();
            trace!("Found block device {block_device}");
            if block_device.is_empty() {
                continue;
            }
            list.push(entry.path());
        }
        Ok(list)
    }

    pub fn display_name(&self) -> String {
        let capacity_formatted = convert_storage(self.capacity().unwrap_or_default() as f64, true);
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
    pub fn sys_stats(&self) -> Result<HashMap<String, usize>> {
        let stat = std::fs::read_to_string(self.sysfs_path.join("stat"))
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

    fn drive_type(&self) -> Result<DriveType> {
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
            std::fs::read_to_string(self.sysfs_path.join("queue/rotational"))
        {
            // turn rot into a boolean
            let rotational = rotational
                .replace('\n', "")
                .parse::<u8>()
                .map(|rot| rot != 0)?;
            if rotational {
                Ok(DriveType::Hdd)
            } else if self.removable()? {
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
    pub fn removable(&self) -> Result<bool> {
        std::fs::read_to_string(self.sysfs_path.join("removable"))?
            .replace('\n', "")
            .parse::<u8>()
            .map(|rem| rem != 0)
            .context("unable to parse removable sysfs file")
    }

    /// Returns, whether the drive is writable
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub fn writable(&self) -> Result<bool> {
        std::fs::read_to_string(self.sysfs_path.join("ro"))?
            .replace('\n', "")
            .parse::<u8>()
            .map(|ro| ro == 0)
            .context("unable to parse ro sysfs file")
    }

    /// Returns the capacity of the drive **in bytes**
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub fn capacity(&self) -> Result<u64> {
        std::fs::read_to_string(self.sysfs_path.join("size"))?
            .replace('\n', "")
            .parse::<u64>()
            .map(|sectors| sectors * 512)
            .context("unable to parse size sysfs file")
    }

    /// Returns the model information of the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub fn model(&self) -> Result<String> {
        std::fs::read_to_string(self.sysfs_path.join("device/model"))
            .context("unable to parse model sysfs file")
    }

    /// Returns the Link info of the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are errors during
    /// reading or parsing, or if the drive link type is not supported
    pub fn link(&self) -> Result<Link> {
        match self.slot {
            DriveSlot::Pci(slot) => Ok(Link::Pcie(LinkData::from_pci_slot(&slot)?)),
            DriveSlot::Ata(slot) => Ok(Link::Sata(LinkData::from_ata_slot(&slot)?)),
            _ => bail!("unsupported drive connection type"),
        }
    }

    pub fn slot(&self) -> Result<DriveSlot> {
        if let Ok(pci_slot) = self.pci_slot() {
            Ok(DriveSlot::Pci(pci_slot))
        } else if let Ok(ata_slot) = self.ata_slot() {
            Ok(DriveSlot::Ata(ata_slot))
        } else {
            bail!("unsupported drive slot type")
        }
    }

    fn pci_slot(&self) -> Result<PciSlot> {
        let pci_address_path = self.sysfs_path.join("device").join("address");
        let pci_address =
            std::fs::read_to_string(pci_address_path).map(|x| x.trim().to_string())?;

        Ok(PciSlot::from_str(&pci_address)?)
    }

    fn ata_slot(&self) -> Result<AtaSlot> {
        let symlink = std::fs::read_link(&self.sysfs_path)
            .context("Could not read sysfs_path as symlink")?
            .to_string_lossy()
            .to_string();
        // ../../devices/pci0000:40/0000:40:08.3/0000:47:00.0/ata25/host24/target24:0:0/24:0:0:0/block/sda

        let ata_sub_path_match = RE_ATA_SLOT
            .captures(&symlink)
            .context("No ata match found, probably no ata device")?;

        let ata_sub_path = ata_sub_path_match
            .get(1)
            .context("No ata match found, probably no ata device")?
            .as_str();

        let ata_device = ata_sub_path_match
            .get(2)
            .context("could not match digits in ata")?
            .as_str()
            .parse::<u8>()?;

        let ata_path = Path::new(&self.sysfs_path).join("..").join(ata_sub_path);
        let dot_parsed_path = ata_path.parse_dot()?.clone();
        let sub_dirs = std::fs::read_dir(dot_parsed_path).context("Could not read ata path")?;

        let ata_link = sub_dirs
            .filter_map(|x| {
                x.ok().and_then(|x| {
                    RE_ATA_LINK
                        .captures(&x.file_name().to_string_lossy())
                        .and_then(|captures| captures.get(2))
                        .and_then(|capture| capture.as_str().parse::<u8>().ok())
                })
            })
            .next()
            .context("No ata link number found")?;

        Ok(AtaSlot {
            ata_device,
            ata_link,
        })
    }

    /// Returns the World-Wide Identification of the drive
    ///
    /// # Errors
    ///
    /// Will return `Err` if the are errors during
    /// reading or parsing
    pub fn wwid(&self) -> Result<String> {
        std::fs::read_to_string(self.sysfs_path.join("device/wwid"))
            .context("unable to parse wwid sysfs file")
    }

    /// Returns the appropriate Icon for the type of drive
    pub fn icon(&self) -> Icon {
        match self.drive_type {
            DriveType::CdDvdBluray => ThemedIcon::new("cd-dvd-bluray-symbolic").into(),
            DriveType::Emmc => ThemedIcon::new("emmc-symbolic").into(),
            DriveType::Flash => ThemedIcon::new("flash-storage-symbolic").into(),
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

    pub fn is_virtual(&self) -> bool {
        match self.drive_type {
            DriveType::LoopDevice
            | DriveType::MappedDevice
            | DriveType::Raid
            | DriveType::RamDisk
            | DriveType::ZfsVolume
            | DriveType::Zram => true,
            _ => self.capacity().unwrap_or(0) == 0,
        }
    }

    pub fn default_icon() -> Icon {
        ThemedIcon::new("unknown-drive-type-symbolic").into()
    }
}
