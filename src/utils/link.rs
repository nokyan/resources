use crate::i18n::{i18n, i18n_f};
use crate::utils::drive::{AtaSlot, UsbSlot};
use crate::utils::link::SataSpeed::{Sata150, Sata300, Sata600};
use crate::utils::network::{InterfaceType, NetworkInterface};
use crate::utils::units::{
    convert_frequency, convert_speed_bits_decimal, convert_speed_bits_decimal_with_places,
};
use anyhow::{Context, Error, Result, anyhow, bail};
use log::{info, trace};
use neli_wifi::{Socket, Station};
use plotters::prelude::LogScalable;
use process_data::pci_slot::PciSlot;
use std::ffi::CString;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Default)]
pub enum Link {
    Pcie(LinkData<PcieLinkData>),
    Sata(LinkData<SataSpeed>),
    Usb(LinkData<UsbSpeed>),
    Wifi(LinkData<WifiGeneration>),
    #[default]
    Unknown,
}

#[derive(Debug)]
pub struct LinkData<T> {
    pub current: T,
    pub max: Result<T>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PcieLinkData {
    pub speed: PcieSpeed,
    pub width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PcieSpeed {
    Pcie10,
    Pcie20,
    Pcie30,
    Pcie40,
    Pcie50,
    Pcie60,
    Pcie70,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SataSpeed {
    Sata150,
    Sata300,
    Sata600,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkLinkData {
    Wifi(WifiLinkData),
    Other(usize),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WifiLinkData {
    pub generation: Option<WifiGeneration>,
    pub frequency_mhz: u32,
    pub rx_bps: usize,
    pub tx_bps: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WifiGeneration {
    Wifi4,
    Wifi5,
    Wifi6,
    Wifi6e,
    Wifi7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UsbSpeed {
    // https://en.wikipedia.org/wiki/USB#Release_versions
    Usb1_0,
    Usb1_1(usize),
    Usb2_0(usize),
    Usb3_0(usize),
    Usb3_1(usize),
    Usb3_2(usize),
    Usb4(usize),
    Usb4_2_0(usize),
}

impl LinkData<PcieLinkData> {
    pub fn from_pci_slot(pci_slot: &PciSlot) -> Result<Self> {
        let pcie_dir = format!("/sys/bus/pci/devices/{pci_slot}/");
        let pcie_folder = Path::new(pcie_dir.as_str());
        if pcie_folder.exists() {
            return Self::read_pcie_link_data(pcie_folder);
        }
        bail!("Could not find PCIe address entry for {pci_slot}");
    }

    fn read_pcie_link_data<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        trace!("Reading PCIe link data for {path:?}…");

        let current_pcie_speed_raw = std::fs::read_to_string(path.join("current_link_speed"))
            .map(|x| x.trim().to_string())
            .context("Could not read current link speed")?;

        let current_pcie_width_raw = std::fs::read_to_string(path.join("current_link_width"))
            .map(|x| x.trim().to_string())
            .context("Could not read current link width")?;

        //Consider max values as optional
        let max_pcie_speed_raw = std::fs::read_to_string(path.join("max_link_speed"))
            .map(|x| x.trim().to_string())
            .context("Could not read max link speed");

        let max_pcie_width_raw = std::fs::read_to_string(path.join("max_link_width"))
            .map(|x| x.trim().to_string())
            .context("Could not read max link width");

        let current = PcieLinkData::parse(&current_pcie_speed_raw, &current_pcie_width_raw)
            .context("Could not parse PCIE link data")?;
        let max = if let (Ok(speed), Ok(width)) = (max_pcie_speed_raw, max_pcie_width_raw) {
            PcieLinkData::parse(&speed, &width)
        } else {
            Err(anyhow!("Could not parse max PCIe link"))
        };
        Ok(Self { current, max })
    }
}

impl PcieLinkData {
    pub fn parse<S: AsRef<str>>(speed_raw: S, width_raw: S) -> Result<Self> {
        let speed = PcieSpeed::from_str(speed_raw.as_ref())?;
        let width = width_raw
            .as_ref()
            .parse::<usize>()
            .context("Could not parse PCIe width")?;
        Ok(PcieLinkData { speed, width })
    }
}

impl<T> Display for LinkData<T>
where
    T: Display,
    T: Copy,
    T: PartialEq,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let has_different_max = {
            if let Ok(max) = self.max {
                self.current != max
            } else {
                false
            }
        };
        if has_different_max {
            write!(f, "{} / {}", self.current, self.max.as_ref().unwrap())
        } else {
            write!(f, "{}", self.current)
        }
    }
}

impl LinkData<SataSpeed> {
    pub fn from_ata_slot(ata_slot: &AtaSlot) -> Result<Self> {
        trace!("Reading ATA link data for {ata_slot:?}…");

        let ata_link_path =
            Path::new("/sys/class/ata_link").join(format!("link{}", ata_slot.ata_link));

        let current_sata_speed_raw = std::fs::read_to_string(ata_link_path.join("sata_spd"))
            .map(|x| x.trim().to_string())
            .context("Could not read sata_spd")?;

        let max_sata_speed_raw = std::fs::read_to_string(ata_link_path.join("sata_spd_max"))
            .map(|x| x.trim().to_string())
            .context("Could not read sata_spd_max");

        let current = SataSpeed::from_str(&current_sata_speed_raw)
            .context("Could not parse current sata speed")?;
        let max = max_sata_speed_raw.and_then(|raw| SataSpeed::from_str(&raw));

        Ok(Self { current, max })
    }
}

impl LinkData<UsbSpeed> {
    pub fn from_usb_slot(usb_slot: &UsbSlot) -> Result<Self> {
        trace!("Reading USB link data for {usb_slot:?}…");

        let usb_bus_path =
            Path::new("/sys/bus/usb/devices/").join(format!("usb{}", usb_slot.usb_bus));

        let max_usb_port_speed_raw = std::fs::read_to_string(usb_bus_path.join("speed"))
            .map(|x| x.trim().to_string())
            .context("Could not read usb port speed");

        let usb_device_speed =
            std::fs::read_to_string(usb_bus_path.join(&usb_slot.usb_device).join("speed"))
                .map(|x| x.trim().to_string())
                .context("Could not read usb device speed")?;

        let usb_port_speed = max_usb_port_speed_raw.and_then(|x| UsbSpeed::from_str(&x));

        let usb_device_speed =
            UsbSpeed::from_str(&usb_device_speed).context("Could not parse USB device speed")?;

        Ok(LinkData {
            current: usb_device_speed,
            max: usb_port_speed,
        })
    }
}

impl Display for PcieLinkData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ×{}", self.speed, self.width)
    }
}

impl Display for PcieSpeed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PcieSpeed::Pcie10 => "PCIe 1.0",
                PcieSpeed::Pcie20 => "PCIe 2.0",
                PcieSpeed::Pcie30 => "PCIe 3.0",
                PcieSpeed::Pcie40 => "PCIe 4.0",
                PcieSpeed::Pcie50 => "PCIe 5.0",
                PcieSpeed::Pcie60 => "PCIe 6.0",
                PcieSpeed::Pcie70 => "PCIe 7.0",
            }
        )
    }
}
impl FromStr for PcieSpeed {
    type Err = Error;

    /// https://en.wikipedia.org/wiki/PCI_Express#Comparison_table
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "2.5 GT/s PCIe" => Ok(PcieSpeed::Pcie10),
            "5.0 GT/s PCIe" => Ok(PcieSpeed::Pcie20),
            "8.0 GT/s PCIe" => Ok(PcieSpeed::Pcie30),
            "16.0 GT/s PCIe" => Ok(PcieSpeed::Pcie40),
            "32.0 GT/s PCIe" => Ok(PcieSpeed::Pcie50),
            "64.0 GT/s PCIe" => Ok(PcieSpeed::Pcie60),
            "128.0 GT/s PCIe" => Ok(PcieSpeed::Pcie70),
            _ => Err(anyhow!("Could not parse PCIe speed: '{s}'")),
        }
    }
}

impl FromStr for SataSpeed {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            // https://en.wikipedia.org/wiki/SATA
            "1.5 Gbps" => Ok(Sata150),
            "3.0 Gbps" => Ok(Sata300),
            "6.0 Gbps" => Ok(Sata600),
            _ => Err(anyhow!("Could not parse SATA speed: '{s}'")),
        }
    }
}

impl Display for SataSpeed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Sata150 => "SATA-150",
                Sata300 => "SATA-300",
                Sata600 => "SATA-600",
            }
        )
    }
}

impl Display for UsbSpeed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({})",
            // https://en.wikipedia.org/wiki/USB#Release_versions
            match self {
                UsbSpeed::Usb1_0 => "USB 1.0",
                UsbSpeed::Usb1_1(_) => "USB 1.1",
                UsbSpeed::Usb2_0(_) => "USB 2.0",
                UsbSpeed::Usb3_0(_) => "USB 3.0",
                UsbSpeed::Usb3_1(_) => "USB 3.1",
                UsbSpeed::Usb3_2(_) => "USB 3.2",
                UsbSpeed::Usb4(_) => "USB4",
                UsbSpeed::Usb4_2_0(_) => "USB4 2.0",
            },
            match self {
                UsbSpeed::Usb1_0 => convert_speed_bits_decimal_with_places(1.5 * 1_000_000.0, 1),
                UsbSpeed::Usb1_1(mbit)
                | UsbSpeed::Usb2_0(mbit)
                | UsbSpeed::Usb3_0(mbit)
                | UsbSpeed::Usb3_1(mbit)
                | UsbSpeed::Usb3_2(mbit)
                | UsbSpeed::Usb4(mbit)
                | UsbSpeed::Usb4_2_0(mbit) =>
                    convert_speed_bits_decimal_with_places(*mbit as f64 * 1_000_000.0, 0),
            }
        )
    }
}
impl Display for WifiGeneration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                WifiGeneration::Wifi4 => "Wi-Fi 4 (802.11n)",
                WifiGeneration::Wifi5 => "Wi-Fi 5 (802.11ac)",
                WifiGeneration::Wifi6 => "Wi-Fi 6 (802.11ax)",
                WifiGeneration::Wifi6e => "Wi-Fi 6E (802.11ax)",
                WifiGeneration::Wifi7 => "Wi-Fi 7 (802.11be)",
            }
        )
    }
}

impl WifiGeneration {
    pub fn get_wifi_generation(station: &Station, frequency_mhz: u32) -> Option<WifiGeneration> {
        let mut wifi_generation: Option<WifiGeneration> = None;

        if station.ht_mcs.is_some() {
            wifi_generation = Some(WifiGeneration::Wifi4)
        }
        if station.vht_mcs.is_some() {
            wifi_generation = Some(WifiGeneration::Wifi5)
        }
        if station.he_mcs.is_some() {
            if frequency_mhz >= 5925 && frequency_mhz <= 7125 {
                wifi_generation = Some(WifiGeneration::Wifi6e)
            } else {
                wifi_generation = Some(WifiGeneration::Wifi6)
            }
        }
        if station.eht_mcs.is_some() {
            wifi_generation = Some(WifiGeneration::Wifi7)
        }
        wifi_generation
    }
}
impl FromStr for UsbSpeed {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            // https://en.wikipedia.org/wiki/USB#Release_versions
            //https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-bus-usb
            "1.5" => Ok(UsbSpeed::Usb1_0),
            "12" => Ok(UsbSpeed::Usb1_1(12)),
            "480" => Ok(UsbSpeed::Usb2_0(480)),
            "5000" => Ok(UsbSpeed::Usb3_0(5_000)),
            "10000" => Ok(UsbSpeed::Usb3_1(10_000)),
            "20000" => Ok(UsbSpeed::Usb3_2(20_000)),
            "40000" => Ok(UsbSpeed::Usb4(40_000)),
            "80000" => Ok(UsbSpeed::Usb4_2_0(80_000)),
            "120000" => Ok(UsbSpeed::Usb4_2_0(120_000)),
            _ => Err(anyhow!("Could not parse USB speed: '{s}'")),
        }
    }
}

impl LinkData<WifiLinkData> {
    pub fn from_wifi_adapter(interface: &NetworkInterface) -> Result<Self> {
        if interface.interface_type != InterfaceType::Wlan {
            bail!("Wifi interface type is required for wifi generation detection");
        }
        let name = interface
            .interface_name
            .to_str()
            .ok_or(anyhow!("No name"))?;
        let mut socket = Socket::connect()?;
        let interfaces = socket
            .get_interfaces_info()
            .context("Could not get interfaces")?;
        let wifi_interface = interfaces
            .iter()
            .filter(|x| {
                x.name.is_some() && {
                    if let Ok(c_name) = CString::from_vec_with_nul(x.name.clone().unwrap()) {
                        c_name.to_string_lossy() == name
                    } else {
                        false
                    }
                }
            })
            .next();
        if let Some(wifi_interface) = wifi_interface {
            let wifi_interface_name =
                String::from_utf8_lossy(wifi_interface.name.as_ref().unwrap());
            info!("Found interface '{}': {:?}", wifi_interface_name, interface);
            let index = wifi_interface
                .index
                .ok_or(anyhow!("Could not get index of wifi_interface"))?;
            let stations = socket.get_station_info(index)?;
            info!("Stations found: {}", stations.len());
            if let Some(station_info) = stations.first() {
                info!("Found station: {:?}", station_info,);
                let mhz = wifi_interface.frequency.unwrap_or(0);
                let wifi_generation = WifiGeneration::get_wifi_generation(station_info, mhz);
                let rx = station_info.rx_bitrate.unwrap_or(0).saturating_mul(100_000) as usize;
                let tx = station_info.tx_bitrate.unwrap_or(0).saturating_mul(100_000) as usize;
                return Ok(LinkData {
                    current: WifiLinkData {
                        generation: wifi_generation,
                        rx_bps: rx,
                        tx_bps: tx,
                        frequency_mhz: wifi_interface.frequency.unwrap_or(0),
                    },
                    max: Err(anyhow!("No max yet supported")),
                });
            }
        }
        bail!("Could not find matching WIFI interface");
    }
}

impl WifiLinkData {
    pub fn frequency_display(&self) -> String {
        // https://en.wikipedia.org/wiki/List_of_WLAN_channels
        match self.frequency_mhz {
            0 => "".to_string(),
            2400..=2495 => "2.4 GHz".to_string(),
            5150..=5895 => "5 GHz".to_string(),
            5925..=7125 => "6 GHz".to_string(),
            _ => convert_frequency((self.frequency_mhz.as_f64() / 1_000.0) * 1_000.0 * 1_000_000.0),
        }
    }

    pub fn link_speed_display(&self) -> String {
        let send_string = convert_speed_bits_decimal(self.tx_bps.as_f64());
        let receive_string = convert_speed_bits_decimal(self.rx_bps.as_f64());

        i18n_f(
            "{}: {} · {}: {}",
            &[
                &i18n("Receive"),
                &receive_string,
                &i18n("Send"),
                &send_string,
            ],
        )
    }
}
impl Display for WifiLinkData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} · {}",
            if let Some(generation) = self.generation {
                generation.to_string()
            } else {
                i18n("N/A")
            },
            self.frequency_display()
        )
    }
}

impl Display for Link {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Link::Pcie(data) => data.to_string(),
                Link::Sata(data) => data.to_string(),
                Link::Usb(data) => data.to_string(),
                Link::Wifi(data) => data.to_string(),
                Link::Unknown => i18n("N/A"),
            }
        )
    }
}

#[cfg(test)]
mod test {
    use crate::utils::link::{
        LinkData, PcieLinkData, PcieSpeed, SataSpeed, UsbSpeed, WifiGeneration, WifiLinkData,
    };
    use anyhow::anyhow;
    use neli_wifi::Station;
    use std::collections::HashMap;
    use std::str::FromStr;

    #[test]
    fn parse_pcie_link_speeds() {
        let map = HashMap::from([
            ("2.5 GT/s PCIe", PcieSpeed::Pcie10),
            ("5.0 GT/s PCIe", PcieSpeed::Pcie20),
            ("8.0 GT/s PCIe", PcieSpeed::Pcie30),
            ("16.0 GT/s PCIe", PcieSpeed::Pcie40),
            ("32.0 GT/s PCIe", PcieSpeed::Pcie50),
            ("64.0 GT/s PCIe", PcieSpeed::Pcie60),
            ("128.0 GT/s PCIe", PcieSpeed::Pcie70),
        ]);

        for input in map.keys() {
            let result = PcieSpeed::from_str(input);
            assert!(result.is_ok(), "Could not parse PCIe speed for '{input}'");
            let expected = map[input];
            pretty_assertions::assert_eq!(expected, result.unwrap());
        }
    }

    #[test]
    fn parse_pcie_link_speeds_failure() {
        let invalid = vec!["256.0 GT/s PCIe", "SOMETHING_ELSE", ""];

        for input in invalid {
            let result = PcieSpeed::from_str(input);
            assert!(
                result.is_err(),
                "Could parse PCIe speed for '{input}' while we don't expect that"
            );
        }
    }

    #[test]
    fn display_pcie_link_speeds() {
        let map = HashMap::from([
            (PcieSpeed::Pcie10, "PCIe 1.0"),
            (PcieSpeed::Pcie20, "PCIe 2.0"),
            (PcieSpeed::Pcie30, "PCIe 3.0"),
            (PcieSpeed::Pcie40, "PCIe 4.0"),
            (PcieSpeed::Pcie50, "PCIe 5.0"),
            (PcieSpeed::Pcie60, "PCIe 6.0"),
            (PcieSpeed::Pcie70, "PCIe 7.0"),
        ]);

        for input in map.keys() {
            let result = input.to_string();
            let expected = map[input];
            pretty_assertions::assert_str_eq!(expected, result);
        }
    }

    #[test]
    fn display_pcie_link_data() {
        let map = get_test_pcie_link_data();

        for input in map.keys() {
            let result = input.to_string();
            let expected = map[input];
            pretty_assertions::assert_str_eq!(expected, result);
        }
    }

    #[test]
    fn parse_pcie_link_data_invalid_input() {
        let result = PcieLinkData::parse("random", "noise");
        assert!(result.is_err());
    }

    #[test]
    fn display_pcie_link_identical_current_max_only_once() {
        let map = get_test_pcie_link_data();

        for link_data in map.keys() {
            let input = LinkData {
                current: *link_data,
                max: Ok(*link_data),
            };
            let result = input.to_string();
            let expected = map[link_data];
            pretty_assertions::assert_str_eq!(expected, result);
        }
    }

    #[test]
    fn display_pcie_link_no_max() {
        let map = get_test_pcie_link_data();

        for link_data in map.keys() {
            let input = LinkData {
                current: *link_data,
                max: Err(anyhow!("No max")),
            };
            let result = input.to_string();
            let expected = map[link_data];
            pretty_assertions::assert_str_eq!(expected, result);
        }
    }

    #[test]
    fn display_pcie_link_different_max() {
        let map = get_test_pcie_link_data();

        for current_data in map.keys() {
            for max_data in map.keys() {
                if current_data != max_data {
                    let input = LinkData {
                        current: *current_data,
                        max: Ok(*max_data),
                    };
                    let result = input.to_string();
                    let expected = format!("{} / {}", current_data, max_data);
                    pretty_assertions::assert_str_eq!(expected, result);
                }
            }
        }
    }

    #[test]
    fn display_pcie_link_different_max_2() {
        let input = LinkData {
            current: PcieLinkData {
                speed: PcieSpeed::Pcie40,
                width: 8,
            },
            max: Ok(PcieLinkData {
                speed: PcieSpeed::Pcie50,
                width: 16,
            }),
        };
        let result = input.to_string();
        let expected = "PCIe 4.0 ×8 / PCIe 5.0 ×16";
        pretty_assertions::assert_str_eq!(expected, result);
    }

    fn get_test_pcie_link_data() -> HashMap<PcieLinkData, &'static str> {
        HashMap::from([
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie10,
                    width: 2,
                },
                "PCIe 1.0 ×2",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie20,
                    width: 4,
                },
                "PCIe 2.0 ×4",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie30,
                    width: 1,
                },
                "PCIe 3.0 ×1",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie40,
                    width: 8,
                },
                "PCIe 4.0 ×8",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie50,
                    width: 16,
                },
                "PCIe 5.0 ×16",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie60,
                    width: 1,
                },
                "PCIe 6.0 ×1",
            ),
        ])
    }

    #[test]
    fn parse_sata_link_speeds() {
        let map = HashMap::from([
            ("1.5 Gbps", SataSpeed::Sata150),
            ("3.0 Gbps", SataSpeed::Sata300),
            ("6.0 Gbps", SataSpeed::Sata600),
        ]);

        for input in map.keys() {
            let result = SataSpeed::from_str(input);
            assert!(result.is_ok(), "Could not parse SATA speed for '{input}'");
            let expected = map[input];
            pretty_assertions::assert_eq!(expected, result.unwrap());
        }
    }

    #[test]
    fn parse_sata_link_speeds_failure() {
        let invalid = vec!["4.0 Gbps", "SOMETHING_ELSE", ""];

        for input in invalid {
            let result = SataSpeed::from_str(input);
            assert!(
                result.is_err(),
                "Could parse SATA speed for '{input}' while we don't expect that"
            );
        }
    }

    #[test]
    fn display_sata_link_speeds() {
        let map = HashMap::from([
            (SataSpeed::Sata150, "SATA-150"),
            (SataSpeed::Sata300, "SATA-300"),
            (SataSpeed::Sata600, "SATA-600"),
        ]);

        for input in map.keys() {
            let result = input.to_string();
            let expected = map[input];
            pretty_assertions::assert_str_eq!(expected, result);
        }
    }

    #[test]
    fn parse_usb_link_speeds() {
        let map = HashMap::from([
            ("1.5", UsbSpeed::Usb1_0),
            ("12", UsbSpeed::Usb1_1(12)),
            ("480", UsbSpeed::Usb2_0(480)),
            ("5000", UsbSpeed::Usb3_0(5_000)),
            ("10000", UsbSpeed::Usb3_1(10_000)),
            ("20000", UsbSpeed::Usb3_2(20_000)),
            ("40000", UsbSpeed::Usb4(40_000)),
            ("80000", UsbSpeed::Usb4_2_0(80_000)),
            ("120000", UsbSpeed::Usb4_2_0(120_000)),
        ]);

        for input in map.keys() {
            let result = UsbSpeed::from_str(input);
            assert!(result.is_ok(), "Could not parse USB speed for '{input}'");
            let expected = map[input];
            pretty_assertions::assert_eq!(expected, result.unwrap());
        }
    }

    #[test]
    fn parse_usb_link_speeds_failure() {
        let invalid = vec!["4000", "160000", "SOMETHING_ELSE", ""];

        for input in invalid {
            let result = UsbSpeed::from_str(input);
            assert!(
                result.is_err(),
                "Could parse USB speed for '{input}' while we don't expect that"
            );
        }
    }

    #[test]
    fn display_usb_link_speeds() {
        let map = HashMap::from([
            (UsbSpeed::Usb1_0, "USB 1.0 (1.5 Mb/s)"),
            (UsbSpeed::Usb1_1(12), "USB 1.1 (12 Mb/s)"),
            (UsbSpeed::Usb2_0(480), "USB 2.0 (480 Mb/s)"),
            (UsbSpeed::Usb3_0(5_000), "USB 3.0 (5 Gb/s)"),
            (UsbSpeed::Usb3_1(10_000), "USB 3.1 (10 Gb/s)"),
            (UsbSpeed::Usb3_2(20_000), "USB 3.2 (20 Gb/s)"),
            (UsbSpeed::Usb4(40_000), "USB4 (40 Gb/s)"),
            (UsbSpeed::Usb4_2_0(80_000), "USB4 2.0 (80 Gb/s)"),
            (UsbSpeed::Usb4_2_0(120_000), "USB4 2.0 (120 Gb/s)"),
        ]);

        for input in map.keys() {
            let result = input.to_string();
            let expected = map[input];
            pretty_assertions::assert_str_eq!(expected, result);
        }
    }

    #[test]
    fn parse_wifi_generations() {
        let map: HashMap<WifiGeneration, (Station, u32)> = HashMap::from([
            (
                WifiGeneration::Wifi4,
                (generate_wifi_station(Some(1), None, None, None), 2400),
            ),
            (
                WifiGeneration::Wifi5,
                (generate_wifi_station(None, Some(1), None, None), 2400),
            ),
            (
                WifiGeneration::Wifi6,
                (generate_wifi_station(None, None, Some(1), None), 2400),
            ),
            (
                WifiGeneration::Wifi6e,
                (generate_wifi_station(None, None, Some(1), None), 6000),
            ),
            (
                WifiGeneration::Wifi7,
                (generate_wifi_station(None, None, None, Some(1)), 5000),
            ),
        ]);

        for expected in map.keys() {
            let (station, mhz) = &map[expected];
            let result = WifiGeneration::get_wifi_generation(station, *mhz);
            assert_eq!(
                result,
                Some(*expected),
                "Could not parse Wifi generation properly for {}",
                expected
            );
        }
    }

    #[test]
    fn parse_wifi_generations_detection_order() {
        let map: HashMap<WifiGeneration, (Station, u32)> = HashMap::from([
            (
                WifiGeneration::Wifi4,
                (generate_wifi_station(Some(1), None, None, None), 2400),
            ),
            (
                WifiGeneration::Wifi5,
                (generate_wifi_station(Some(1), Some(1), None, None), 2400),
            ),
            (
                WifiGeneration::Wifi6,
                (generate_wifi_station(Some(1), Some(1), Some(1), None), 2400),
            ),
            (
                WifiGeneration::Wifi6e,
                (generate_wifi_station(Some(1), Some(1), Some(1), None), 6000),
            ),
            (
                WifiGeneration::Wifi7,
                (
                    generate_wifi_station(Some(1), Some(1), Some(1), Some(1)),
                    5000,
                ),
            ),
        ]);

        for expected in map.keys() {
            let (station, mhz) = &map[expected];
            let result = WifiGeneration::get_wifi_generation(station, *mhz);
            assert_eq!(
                result,
                Some(*expected),
                "Could not parse Wifi generation properly for {}",
                expected
            );
        }
    }

    #[test]
    fn parse_wifi_generations_failures() {
        let map = HashMap::from([(None, (generate_wifi_station(None, None, None, None), 2400))]);

        for expected in map.keys() {
            let (station, mhz) = &map[expected];
            let result = WifiGeneration::get_wifi_generation(station, *mhz);
            assert_eq!(
                result, *expected,
                "Could parse Wifi generation properly while it should fail"
            )
        }
    }

    #[test]
    fn display_wifi_generations() {
        let map: HashMap<WifiGeneration, &str> = HashMap::from([
            (WifiGeneration::Wifi4, "Wi-Fi 4 (802.11n)"),
            (WifiGeneration::Wifi5, "Wi-Fi 5 (802.11ac)"),
            (WifiGeneration::Wifi6, "Wi-Fi 6 (802.11ax)"),
            (WifiGeneration::Wifi6e, "Wi-Fi 6E (802.11ax)"),
            (WifiGeneration::Wifi7, "Wi-Fi 7 (802.11be)"),
        ]);
        for input in map.keys() {
            let result = input.to_string();
            let expected = map[input];
            pretty_assertions::assert_str_eq!(expected, result);
        }
    }

    #[test]
    fn display_wifi_link_frequencies() {
        let map = HashMap::from([
            (2401u32..=2495u32, "2.4 GHz"),
            (5150u32..=5895, "5 GHz"),
            (5925u32..=7125, "6 GHz"),
        ]);
        for mhz_range in map.keys() {
            for step in mhz_range.clone().into_iter() {
                let input = WifiLinkData {
                    generation: None,
                    frequency_mhz: step,
                    rx_bps: 0,
                    tx_bps: 0,
                };
                let result = input.frequency_display();
                let expected = map[mhz_range];
                pretty_assertions::assert_eq!(expected, result);
            }
        }
    }

    #[test]
    fn display_unsupported_wifi_link_frequencies() {
        let map = HashMap::from([
            (2400, "2.4 GHz"),
            (5000, "5.00 GHz"),
            (8000, "8.00 GHz"),
            (8123, "8.12 GHz"),
            (0, ""),
        ]);
        for mhz in map.keys() {
            let input = WifiLinkData {
                generation: None,
                frequency_mhz: *mhz,
                rx_bps: 0,
                tx_bps: 0,
            };
            let result = input.frequency_display();
            let expected = map[mhz];
            pretty_assertions::assert_eq!(expected, result);
        }
    }

    #[test]
    fn display_wifi_link_speed() {
        let map = HashMap::from([
            ("Receive: 200 b/s · Send: 100 b/s", (200, 100)),
            ("Receive: 200 kb/s · Send: 100 kb/s", (200_000, 100_000)),
            (
                "Receive: 200 Mb/s · Send: 100 Mb/s",
                (200_000_000, 100_000_000),
            ),
            (
                "Receive: 235 Mb/s · Send: 124 Mb/s",
                (235_000_000, 124_000_000),
            ),
            ("Receive: 2 kb/s · Send: 124 Mb/s", (2_000, 124_000_000)),
            ("Receive: 124 Mb/s · Send: 2 kb/s", (124_000_000, 2_000)),
        ]);

        for expected in map.keys() {
            let (receive, send) = map[expected];
            let input = WifiLinkData {
                generation: None,
                frequency_mhz: 0,
                rx_bps: receive,
                tx_bps: send,
            };
            let result = input.link_speed_display();
            pretty_assertions::assert_eq!(*expected, result);
        }
    }
    fn generate_wifi_station(
        ht_mcs: Option<u8>,
        vht_mcs: Option<u8>,
        he_mcs: Option<u8>,
        eht_mcs: Option<u8>,
    ) -> Station {
        let mut station = Station::default();
        station.ht_mcs = ht_mcs;
        station.vht_mcs = vht_mcs;
        station.he_mcs = he_mcs;
        station.eht_mcs = eht_mcs;
        station
    }
}
