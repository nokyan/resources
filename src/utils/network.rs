use std::{
    ffi::OsString,
    fmt::Display,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use gtk::gio::{Icon, ThemedIcon};
use log::trace;

use super::{pci::Device, read_uevent};
use crate::i18n::i18n;
use crate::utils::link::{LinkData, NetworkLinkData, WifiLinkData};

const PATH_SYSFS: &str = "/sys/class/net";

// this is a list because we don't look for exact matches but for if the device name starts with a certain string
const INTERFACE_TYPE_MAP: &[(&str, InterfaceType)] = &[
    ("bn", InterfaceType::Bluetooth),
    ("br", InterfaceType::Bridge),
    ("dae", InterfaceType::Vpn),
    ("docker", InterfaceType::Docker),
    ("eth", InterfaceType::Ethernet),
    ("en", InterfaceType::Ethernet),
    ("ib", InterfaceType::InfiniBand),
    ("sl", InterfaceType::Slip),
    ("tun", InterfaceType::Vpn),
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
    pub link: Result<LinkData<WifiLinkData>>,
    pub link_speed: Result<NetworkLinkData>,
}

impl NetworkData {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();

        trace!("Gathering network data for {path:?}…");

        let inner = NetworkInterface::from_sysfs(path);
        let is_virtual = inner.is_virtual();
        let received_bytes = inner.received_bytes();
        let sent_bytes = inner.sent_bytes();
        let display_name = inner.display_name();
        let link: Result<LinkData<WifiLinkData>> = LinkData::from_wifi_adapter(&inner);
        let link_speed = match &link {
            Ok(wifi_link) => Ok(NetworkLinkData::Wifi(wifi_link.current)),
            Err(_) => inner.link_speed(),
        };

        let network_data = Self {
            inner,
            is_virtual,
            received_bytes,
            sent_bytes,
            display_name,
            link,
            link_speed,
        };

        trace!(
            "Gathered network data for {}: {network_data:?}",
            path.to_string_lossy()
        );

        network_data
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
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
        for (name, interface_type) in INTERFACE_TYPE_MAP {
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
    pub device: Option<&'static Device>,
    pub device_label: Option<String>,
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
            && self.device == other.device
            && self.hw_address == other.hw_address
    }
}

impl NetworkInterface {
    pub fn get_sysfs_paths() -> Result<Vec<PathBuf>> {
        let mut list = Vec::new();

        trace!("Finding entries in {PATH_SYSFS}");

        let entries = std::fs::read_dir(PATH_SYSFS)?;
        for entry in entries {
            let entry = entry?;
            let block_device = entry.file_name().to_string_lossy().to_string();
            trace!("Found block device {block_device}");
            if block_device.starts_with("lo") {
                trace!("Skipping loopback interface {block_device}");
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
        trace!("Creating NetworkInterface object of {sysfs_path:?}…");

        let dev_uevent = read_uevent(sysfs_path.join("device/uevent")).unwrap_or_default();

        let interface_name = sysfs_path
            .file_name()
            .expect("invalid sysfs path")
            .to_owned();

        let device = if let Some(pci_line) = dev_uevent.get("PCI_ID") {
            let (vid_str, pid_str) = pci_line.split_once(':').unwrap_or(("0", "0"));
            let vid = u16::from_str_radix(vid_str, 16).unwrap_or_default();
            let pid = u16::from_str_radix(pid_str, 16).unwrap_or_default();
            Device::from_vid_pid(vid, pid)
        } else {
            None
        };

        let sysfs_path_clone = sysfs_path.to_owned();
        let speed = std::fs::read_to_string(sysfs_path_clone.join("speed"))
            .map(|x| x.parse().unwrap_or_default())
            .ok();

        let sysfs_path_clone = sysfs_path.to_owned();
        let device_label = std::fs::read_to_string(sysfs_path_clone.join("device/label"))
            .map(|x| x.replace('\n', ""))
            .ok();

        let sysfs_path_clone = sysfs_path.to_owned();
        let hw_address = std::fs::read_to_string(sysfs_path_clone.join("address"))
            .map(|x| x.replace('\n', ""))
            .ok();

        let interface_type = InterfaceType::from_interface_name(interface_name.to_string_lossy());

        let driver = dev_uevent.get("DRIVER");

        let network_interface = NetworkInterface {
            interface_name: interface_name.clone(),
            driver_name: driver.cloned(),
            interface_type,
            speed,
            device,
            device_label,
            hw_address,
            sysfs_path: sysfs_path.to_path_buf(),
            received_bytes_path: sysfs_path.join(PathBuf::from("statistics/rx_bytes")),
            sent_bytes_path: sysfs_path.join(PathBuf::from("statistics/tx_bytes")),
        };

        trace!("Created NetworkInterface object of {sysfs_path:?}: {network_interface:?}");

        network_interface
    }

    /// Returns a display name for this Network Interface.
    /// It tries to be as human readable as possible.
    pub fn display_name(&self) -> String {
        self.device_label
            .clone()
            .or_else(|| self.device.map(|device| device.name().to_string()))
            .unwrap_or_else(|| self.interface_name.to_string_lossy().to_string())
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

    /// Returns the link speed of this connection in bits per second
    ///
    /// # Errors
    ///
    /// Will return `Err` if the link speed couldn't be determined (e. g. for Wi-Fi connections)
    pub fn link_speed(&self) -> Result<NetworkLinkData> {
        let mpbs = std::fs::read_to_string(self.sysfs_path.join("speed"))
            .context("read failure")?
            .replace('\n', "")
            .parse::<usize>()
            .context("parsing failure")
            .map(|mbps| mbps.saturating_mul(1_000_000))?;
        Ok(NetworkLinkData::Other(mpbs))
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
