use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use pci_ids::FromId;

#[derive(Debug, Clone, Copy)]
pub enum InterfaceType {
    Ethernet,
    InfiniBand,
    Slip,
    Wlan,
    Wwan,
    Bluetooth,
    Other,
}

impl Default for InterfaceType {
    fn default() -> Self {
        InterfaceType::Other
    }
}

#[derive(Debug, Clone, Default)]
/// Represents a network interface found in /sys/class/net
pub struct NetworkInterface {
    pub interface_name: OsString,
    pub driver_name: Option<String>,
    pub interface_type: InterfaceType,
    pub speed: Option<usize>,
    pub vendor: Option<String>,
    pub pid_name: Option<String>,
    pub device_name: Option<String>,
    pub hw_address: Option<String>,
    pub sysfs_path: PathBuf,
    received_bytes_path: PathBuf,
    sent_bytes_path: PathBuf,
}

impl PartialEq for NetworkInterface {
    fn eq(&self, other: &Self) -> bool {
        self.interface_name == other.interface_name
            && self.vendor == other.vendor
            && self.pid_name == other.pid_name
            && self.hw_address == other.hw_address
    }
}

impl NetworkInterface {
    fn read_uevent(uevent_path: PathBuf) -> Result<HashMap<String, String>> {
        let entries: Vec<Vec<String>> = std::fs::read_to_string(uevent_path)?
            .split('\n')
            .map(|x| x.split('=').map(str::to_string).collect())
            .collect();
        let mut hmap = HashMap::new();
        for entry in entries {
            if entry.len() == 2 {
                hmap.insert(entry[0].clone(), entry[1].clone());
            }
        }
        Ok(hmap)
    }

    /// Returns a `NetworkInterface` based on information
    /// found in its sysfs path
    ///
    /// # Errors
    ///
    /// Will return `Err` if an invalid sysfs Path has
    /// been passed or if there has been problems parsing
    /// information
    pub fn from_sysfs(sysfs_path: &Path) -> Result<NetworkInterface> {
        let dev_uevent = Self::read_uevent(sysfs_path.join("device/uevent")).unwrap_or_default();
        let interface_name = sysfs_path
            .file_name()
            .with_context(|| "invalid sysfs path")?
            .to_owned();
        let mut vid_pid = (0, 0);
        if let Some(dev) = dev_uevent.get("PCI_ID") {
            let id_vec: Vec<&str> = dev.split(':').collect();
            if id_vec.len() == 2 {
                vid_pid = (
                    u16::from_str_radix(id_vec[0], 16)?,
                    u16::from_str_radix(id_vec[1], 16)?,
                );
            }
        }
        Ok(NetworkInterface {
            interface_name: interface_name.clone(),
            driver_name: dev_uevent.get("DRIVER").cloned(),
            interface_type: match interface_name
                .to_str()
                .with_context(|| "unable to convert OsString to &str")?[..2]
                .into()
            {
                // this requires systemd's PredictableNetworkInterfaceNames to be active,
                // otherwise it's (probably) just going to be `InterfaceType::Other`
                "en" => InterfaceType::Ethernet,
                "ib" => InterfaceType::InfiniBand,
                "sl" => InterfaceType::Slip,
                "wl" => InterfaceType::Wlan,
                "ww" => InterfaceType::Wwan,
                "bn" => InterfaceType::Bluetooth,
                _ => InterfaceType::Other,
            },
            speed: std::fs::read_to_string(sysfs_path.join("speed"))
                .map(|x| x.parse().unwrap_or_default())
                .ok(),
            vendor: pci_ids::Vendor::from_id(vid_pid.0).map(|x| x.name().to_string()),
            pid_name: pci_ids::Device::from_vid_pid(vid_pid.0, vid_pid.1)
                .map(|x| x.name().to_string()),
            device_name: std::fs::read_to_string(sysfs_path.join("device/label"))
                .map(|x| x.replace('\n', ""))
                .ok(),
            hw_address: std::fs::read_to_string(sysfs_path.join("address"))
                .map(|x| x.replace('\n', ""))
                .ok(),
            sysfs_path: sysfs_path.to_path_buf(),
            received_bytes_path: sysfs_path.join(PathBuf::from("statistics/rx_bytes")),
            sent_bytes_path: sysfs_path.join(PathBuf::from("statistics/tx_bytes")),
        })
    }

    /// Returns a display name for this Network Interface.
    /// It tries to be as human readable as possible.
    pub fn display_name(&self) -> String {
        self.device_name
            .clone()
            .or_else(|| self.pid_name.clone())
            .unwrap_or_else(|| self.interface_name.to_str().unwrap_or_default().to_string())
    }

    /// Returns the amount of bytes sent by this Network
    /// Interface.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the `tx_bytes` file in sysfs
    /// is unreadable or not parsable to a `usize`
    pub fn received_bytes(&self) -> Result<usize> {
        std::fs::read_to_string(&self.received_bytes_path)
            .with_context(|| "read failure")?
            .replace('\n', "")
            .parse()
            .with_context(|| "parsing failure")
    }

    /// Returns the amount of bytes sent by this Network
    /// Interface
    ///
    /// # Errors
    ///
    /// Will return `Err` if the `tx_bytes` file in sysfs
    /// is unreadable or not parsable to a `usize`
    pub fn sent_bytes(&self) -> Result<usize> {
        std::fs::read_to_string(&self.sent_bytes_path)
            .with_context(|| "read failure")?
            .replace('\n', "")
            .parse()
            .with_context(|| "parsing failure")
    }
}
