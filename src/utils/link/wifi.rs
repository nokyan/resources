use crate::i18n::{i18n, i18n_f};
use crate::utils::link::LinkData;
use crate::utils::network::{InterfaceType, NetworkInterface};
use crate::utils::units::{convert_frequency, convert_speed_bits_decimal};
use anyhow::{Context, Result, anyhow, bail};
use log::{debug, trace, warn};
use neli_wifi::{Socket, Station};
use plotters::prelude::LogScalable;
use std::ffi::CString;
use std::fmt::{Display, Formatter};
use std::sync::{LazyLock, Mutex};

static NELI_SOCKET: LazyLock<Result<Mutex<Socket>>> = LazyLock::new(|| {
    Socket::connect()
        .inspect(|_| debug!("Successfully connected to nl80211"))
        .map_err(|e| anyhow!("connection to nl80211 failed: {e}"))
        .inspect_err(|e| warn!("Connection to nl80211 failed, reason: {e}"))
        .map(Mutex::new)
});

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WifiLinkData {
    pub generation: Option<WifiGeneration>,
    pub frequency_mhz: u32,
    pub rx_bps: usize,
    pub tx_bps: usize,
    pub ssid: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WifiGeneration {
    Wifi4,
    Wifi5,
    Wifi6,
    Wifi6e,
    Wifi7,
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
            if (5925..=7125).contains(&frequency_mhz) {
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

impl LinkData<WifiLinkData> {
    pub fn from_wifi_adapter(interface: &NetworkInterface) -> Result<Self> {
        if interface.interface_type != InterfaceType::Wlan {
            bail!("Wifi interface type is required for wifi generation detection");
        }
        let name = interface
            .interface_name
            .to_str()
            .context("unable to turn osstring to string")?;
        let mutex = NELI_SOCKET.as_ref().map_err(|e| anyhow!(e))?;
        let mut socket = mutex
            .try_lock()
            .map_err(|e| anyhow!("unable to lock neli mutex, reason: {e}"))?;
        let interfaces = socket
            .get_interfaces_info()
            .context("Could not get interfaces")?;
        let wifi_interface = interfaces.iter().find(|x| {
            x.name.is_some() && {
                if let Ok(c_name) = CString::from_vec_with_nul(x.name.clone().unwrap()) {
                    c_name.to_string_lossy() == name
                } else {
                    false
                }
            }
        });

        if let Some(wifi_interface) = wifi_interface {
            let wifi_interface_name =
                String::from_utf8_lossy(wifi_interface.name.as_ref().unwrap());
            let ssid = wifi_interface
                .ssid
                .as_ref()
                .map(|s| String::from_utf8_lossy(s).to_string());
            trace!("Found interface '{wifi_interface_name}': {interface:?}");
            let index = wifi_interface
                .index
                .context("Could not get index of wifi_interface")?;
            let stations = socket.get_station_info(index)?;
            trace!("Stations found: {}", stations.len());
            if let Some(station_info) = stations.first() {
                trace!("Found station: {station_info:?}");
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
                        ssid,
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

        format!(
            "{} · {}",
            &i18n_f("Receive: {}", &[&receive_string]),
            &i18n_f("Send: {}", &[&send_string]),
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
            self.frequency_display(),
        )
    }
}

#[cfg(test)]
mod test {
    use crate::utils::link::wifi::{WifiGeneration, WifiLinkData};
    use neli_wifi::Station;
    use std::collections::HashMap;

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
                    ssid: None,
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
                ssid: None,
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
            (
                "Receive: 200.00 kb/s · Send: 100.00 kb/s",
                (200_000, 100_000),
            ),
            (
                "Receive: 200.00 Mb/s · Send: 100.00 Mb/s",
                (200_000_000, 100_000_000),
            ),
            (
                "Receive: 235.00 Mb/s · Send: 124.00 Mb/s",
                (235_000_000, 124_000_000),
            ),
            (
                "Receive: 2.00 kb/s · Send: 124.00 Mb/s",
                (2_000, 124_000_000),
            ),
            (
                "Receive: 124.25 Mb/s · Send: 2.30 kb/s",
                (124_250_000, 2_300),
            ),
        ]);

        for expected in map.keys() {
            let (receive, send) = map[expected];
            let input = WifiLinkData {
                generation: None,
                frequency_mhz: 0,
                rx_bps: receive,
                tx_bps: send,
                ssid: None,
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
