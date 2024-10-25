use std::{
    ffi::OsString,
    fmt::Display,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use gtk::gio::{Icon, ThemedIcon};

use crate::i18n::i18n;

use super::{
    pci::{Device, Vendor},
    read_uevent,
};

// this is a vec because we don't look for exact matches but for if the device name starts with a certain string
const INTERFACE_TYPE_MAP: &[(&'static str, InterfaceType)] = &[
    ("bn", InterfaceType::Bluetooth),
    ("br", InterfaceType::Bridge),
    ("docker", InterfaceType::Docker),
    ("eth", InterfaceType::Ethernet),
    ("en", InterfaceType::Ethernet),
    ("ib", InterfaceType::InfiniBand),
    ("sl", InterfaceType::Slip),
    ("veth", InterfaceType::VirtualEthernet),
    ("virbr", InterfaceType::VmBridge),
    ("vpn", InterfaceType::Vpn),
    ("wg", InterfaceType::Wireguard),
    ("wl", InterfaceType::Wlan),
    ("ww", InterfaceType::Wwan),
];

#[derive(Debug)]
pub struct NetworkData {
    pub inner: NetworkInterface,
    pub is_virtual: bool,
    pub received_bytes: Result<usize>,
    pub sent_bytes: Result<usize>,
    pub display_name: String,
}

impl NetworkData {
    pub fn new(path: &Path) -> Self {
        let inner = NetworkInterface::from_sysfs(path);
        let is_virtual = inner.is_virtual();
        let received_bytes = inner.received_bytes();
        let sent_bytes = inner.sent_bytes();
        let display_name = inner.display_name();

        Self {
            inner,
            is_virtual,
            received_bytes,
            sent_bytes,
            display_name,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum InterfaceType {
    Bluetooth,
    Bridge,
    Docker,
    Ethernet,
    InfiniBand,
    Slip,
    VirtualEthernet,
    VmBridge,
    Vpn,
    Wireguard,
    Wlan,
    Wwan,
    #[default]
    Unknown,
}

impl InterfaceType {
    pub fn from_interface_name<S: AsRef<str>>(interface_name: S) -> Self {
        for (name, interface_type) in INTERFACE_TYPE_MAP.iter() {
            if interface_name.as_ref().starts_with(name) {
                return *interface_type;
            }
        }
        Self::Unknown
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
                InterfaceType::Docker => i18n("Docker Bridge"),
                InterfaceType::InfiniBand => i18n("InfiniBand Connection"),
                InterfaceType::Slip => i18n("Serial Line IP Connection"),
                InterfaceType::VirtualEthernet => i18n("Virtual Ethernet Device"),
                InterfaceType::VmBridge => i18n("VM Network Bridge"),
                InterfaceType::Vpn => i18n("VPN Tunnel"),
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
        let entries = std::fs::read_dir("/sys/class/net")?;
        for entry in entries {
            let entry = entry?;
            let block_device = entry.file_name().to_string_lossy().to_string();
            if block_device.starts_with("lo") {
                continue;
            }
            list.push(entry.path());
        }
        Ok(list)
    }

    /// Returns a `NetworkInterface` based on information
    /// found in its sysfs path
    ///
    /// # Errors
    ///
    /// Will return `Err` if an invalid sysfs Path has
    /// been passed or if there has been problems parsing
    /// information
    pub fn from_sysfs(sysfs_path: &Path) -> NetworkInterface {
        let dev_uevent = read_uevent(sysfs_path.join("device/uevent")).unwrap_or_default();

        let interface_name = sysfs_path
            .file_name()
            .expect("invalid sysfs path")
            .to_owned();

        let mut vid_pid = (None, None);
        if let Some(dev) = dev_uevent.get("PCI_ID") {
            let id_vec: Vec<&str> = dev.split(':').collect();
            if id_vec.len() == 2 {
                vid_pid = (
                    u16::from_str_radix(id_vec[0], 16).ok(),
                    u16::from_str_radix(id_vec[1], 16).ok(),
                );
            }
        }

        let vendor = vid_pid
            .0
            .and_then(|vid| Vendor::from_vid(vid).map(|x| x.name().to_string()));

        let pid_name = vid_pid
            .0
            .zip(vid_pid.1)
            .and_then(|(vid, pid)| Device::from_vid_pid(vid, pid).map(|x| x.name().to_string()));

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

        NetworkInterface {
            interface_name: interface_name.clone(),
            driver_name: dev_uevent.get("DRIVER").cloned(),
            interface_type: InterfaceType::from_interface_name(interface_name.to_string_lossy()),
            speed,
            vendor,
            pid_name,
            device_name,
            hw_address,
            sysfs_path: sysfs_path.to_path_buf(),
            received_bytes_path: sysfs_path.join(PathBuf::from("statistics/rx_bytes")),
            sent_bytes_path: sysfs_path.join(PathBuf::from("statistics/tx_bytes")),
        }
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
            .context("read failure")?
            .replace('\n', "")
            .parse()
            .context("parsing failure")
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
            .context("read failure")?
            .replace('\n', "")
            .parse()
            .context("parsing failure")
    }

    /// Returns the appropriate Icon for the type of drive
    pub fn icon(&self) -> Icon {
        match self.interface_type {
            InterfaceType::Bluetooth => ThemedIcon::new("bluetooth-symbolic").into(),
            InterfaceType::Bridge => ThemedIcon::new("bridge-symbolic").into(),
            InterfaceType::Docker => ThemedIcon::new("docker-bridge-symbolic").into(),
            InterfaceType::Ethernet => ThemedIcon::new("ethernet-symbolic").into(),
            InterfaceType::InfiniBand => ThemedIcon::new("infiniband-symbolic").into(),
            InterfaceType::Slip => ThemedIcon::new("slip-symbolic").into(),
            InterfaceType::VirtualEthernet => ThemedIcon::new("virtual-ethernet").into(),
            InterfaceType::VmBridge => ThemedIcon::new("vm-bridge-symbolic").into(),
            InterfaceType::Vpn | InterfaceType::Wireguard => ThemedIcon::new("vpn-symbolic").into(),
            InterfaceType::Wlan => ThemedIcon::new("wlan-symbolic").into(),
            InterfaceType::Wwan => ThemedIcon::new("wwan-symbolic").into(),
            InterfaceType::Unknown => Self::default_icon(),
        }
    }

    pub fn is_virtual(&self) -> bool {
        matches!(
            self.interface_type,
            InterfaceType::Bridge
                | InterfaceType::Docker
                | InterfaceType::VirtualEthernet
                | InterfaceType::Vpn
                | InterfaceType::VmBridge
                | InterfaceType::Wireguard
        )
    }

    pub fn default_icon() -> Icon {
        ThemedIcon::new("unknown-network-type-symbolic").into()
    }
}
