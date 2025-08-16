use crate::utils::drive::UsbSlot;
use crate::utils::link::LinkData;
use crate::utils::units::convert_speed_bits_decimal_with_places;
use anyhow::{Context, Error, anyhow};
use log::trace;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;

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

impl LinkData<UsbSpeed> {
    pub fn from_usb_slot(usb_slot: &UsbSlot) -> anyhow::Result<Self> {
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

#[cfg(test)]
mod test {
    use crate::utils::link::usb::UsbSpeed;
    use std::collections::HashMap;
    use std::str::FromStr;

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
}
