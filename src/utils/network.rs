use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Display,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use gtk::gio::{Icon, ThemedIcon};
use pci_ids::FromId;

use crate::i18n::i18n;

#[derive(Debug)]
pub struct NetworkData {
    pub inner: NetworkInterface,
    pub is_virtual: bool,
    pub received_bytes: usize,
    pub sent_bytes: usize,
    pub display_name: String,
}

impl NetworkData {
    pub fn new(path: &Path) -> Result<Self> {
        let inner = NetworkInterface::from_sysfs(path)?;
        let is_virtual = inner.is_virtual();
        let received_bytes = inner.received_bytes()?;
        let sent_bytes = inner.sent_bytes()?;
        let display_name = inner.display_name();

        Ok(Self {
            inner,
            is_virtual,
            received_bytes,
            sent_bytes,
            display_name,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum InterfaceType {
    Bluetooth,
    Bridge,
    Ethernet,
    InfiniBand,
    Slip,
    VirtualEthernet,
    VmBridge,
    Wireguard,
    Wlan,
    Wwan,
    #[default]
    Unknown,
}

impl InterfaceType {
    pub fn from_interface_name<S: AsRef<str>>(interface_name: S) -> Self {
        let interface_name = interface_name.as_ref();

        if interface_name.starts_with("bn") {
            Self::Bluetooth
        } else if interface_name.starts_with("eth") || interface_name.starts_with("en") {
            Self::Ethernet
        } else if interface_name.starts_with("ib") {
            Self::InfiniBand
        } else if interface_name.starts_with("sl") {
            Self::Slip
        } else if interface_name.starts_with("veth") {
            Self::VirtualEthernet
        } else if interface_name.starts_with("virbr") {
            Self::VmBridge
        } else if interface_name.starts_with("wg") {
            Self::Wireguard
        } else if interface_name.starts_with("wl") {
            Self::Wlan
        } else if interface_name.starts_with("ww") {
            Self::Wwan
        } else {
            Self::Unknown
        }
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

impl Display for InterfaceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                InterfaceType::Bluetooth => i18n("Bluetooth Tether"),
                InterfaceType::Bridge => i18n("Network Bridge"),
                InterfaceType::Ethernet => i18n("Ethernet Connection"),
                InterfaceType::InfiniBand => i18n("InfiniBand Connection"),
                InterfaceType::Slip => i18n("Serial Line IP Connection"),
                InterfaceType::VirtualEthernet => i18n("Virtual Ethernet Device"),
                InterfaceType::VmBridge => i18n("VM Network Bridge"),
                InterfaceType::Wireguard => i18n("VPN Tunnel (WireGuard)"),
                InterfaceType::Wlan => i18n("Wi-Fi Connection"),
                InterfaceType::Wwan => i18n("WWAN Connection"),
                InterfaceType::Unknown => i18n("Network Interface"),
            }
        )
    }
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
    pub fn get_sysfs_paths() -> Result<Vec<PathBuf>> {
        let mut list = Vec::new();
        let mut entries = std::fs::read_dir("/sys/class/net")?;
        while let Some(entry) = entries.next() {
            let entry = entry?;
            let block_device = entry.file_name().to_string_lossy().to_string();
            if block_device.starts_with("lo") {
                continue;
            }
            list.push(entry.path());
        }
        Ok(list)
    }

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

        let sysfs_path_clone = sysfs_path.to_owned();
        let speed = std::fs::read_to_string(sysfs_path_clone.join("speed"))
            .map(|x| x.parse().unwrap_or_default())
            .ok();

        let sysfs_path_clone = sysfs_path.to_owned();
        let device_name = std::fs::read_to_string(sysfs_path_clone.join("device/label"))
            .map(|x| x.replace('\n', ""))
            .ok();

        let sysfs_path_clone = sysfs_path.to_owned();
        let hw_address = std::fs::read_to_string(sysfs_path_clone.join("address"))
            .map(|x| x.replace('\n', ""))
            .ok();

        Ok(NetworkInterface {
            interface_name: interface_name.clone(),
            driver_name: dev_uevent.get("DRIVER").cloned(),
            interface_type: InterfaceType::from_interface_name(interface_name.to_string_lossy()),
            speed,
            vendor: pci_ids::Vendor::from_id(vid_pid.0).map(|x| x.name().to_string()),
            pid_name: pci_ids::Device::from_vid_pid(vid_pid.0, vid_pid.1)
                .map(|x| x.name().to_string()),
            device_name,
            hw_address,
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

    /// Returns the appropriate Icon for the type of drive
    pub fn icon(&self) -> Icon {
        match self.interface_type {
            InterfaceType::Bluetooth => ThemedIcon::new("bluetooth-symbolic").into(),
            InterfaceType::Bridge => ThemedIcon::new("bridge-symbolic").into(),
            InterfaceType::Ethernet => ThemedIcon::new("ethernet-symbolic").into(),
            InterfaceType::InfiniBand => ThemedIcon::new("infiniband-symbolic").into(),
            InterfaceType::Slip => ThemedIcon::new("slip-symbolic").into(),
            InterfaceType::VirtualEthernet => ThemedIcon::new("virtual-ethernet").into(),
            InterfaceType::VmBridge => ThemedIcon::new("vm-bridge-symbolic").into(),
            InterfaceType::Wireguard => ThemedIcon::new("vpn-symbolic").into(),
            InterfaceType::Wlan => ThemedIcon::new("wlan-symbolic").into(),
            InterfaceType::Wwan => ThemedIcon::new("wwan-symbolic").into(),
            InterfaceType::Unknown => Self::default_icon(),
        }
    }

    pub fn is_virtual(&self) -> bool {
        matches!(
            self.interface_type,
            InterfaceType::Bridge
                | InterfaceType::VmBridge
                | InterfaceType::VirtualEthernet
                | InterfaceType::Wireguard
        )
    }

    pub fn default_icon() -> Icon {
        ThemedIcon::new("unknown-network-type-symbolic").into()
    }
}
