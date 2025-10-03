use crate::utils::link::LinkData;
use anyhow::{Context, Error, anyhow, bail};
use log::trace;
use process_data::pci_slot::PciSlot;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;

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
    Pcie80,
}

impl LinkData<PcieLinkData> {
    pub fn from_pci_slot(pci_slot: &PciSlot) -> anyhow::Result<Self> {
        let pcie_dir = format!("/sys/bus/pci/devices/{pci_slot}/");
        let pcie_folder = Path::new(pcie_dir.as_str());
        if pcie_folder.exists() {
            return Self::read_pcie_link_data(pcie_folder);
        }
        bail!("Could not find PCIe address entry for {pci_slot}");
    }

    fn read_pcie_link_data<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
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
    pub fn parse<S: AsRef<str>>(speed_raw: S, width_raw: S) -> anyhow::Result<Self> {
        let speed = PcieSpeed::from_str(speed_raw.as_ref())?;
        let width = width_raw
            .as_ref()
            .parse::<usize>()
            .context("Could not parse PCIe width")?;
        Ok(PcieLinkData { speed, width })
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
                PcieSpeed::Pcie80 => "PCIe 8.0",
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
            "256.0 GT/s PCIe" => Ok(PcieSpeed::Pcie80),
            _ => Err(anyhow!("Could not parse PCIe speed: '{s}'")),
        }
    }
}
#[cfg(test)]
mod test {
    use crate::utils::link::LinkData;
    use crate::utils::link::pcie::{PcieLinkData, PcieSpeed};
    use anyhow::anyhow;
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
            ("256.0 GT/s PCIe", PcieSpeed::Pcie80),
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
        let invalid = vec!["999.0 GT/s PCIe", "SOMETHING_ELSE", ""];

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
            (PcieSpeed::Pcie80, "PCIe 8.0"),
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
}
