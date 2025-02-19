use crate::i18n::i18n;
use crate::utils::drive::AtaSlot;
use crate::utils::link::SataSpeed::{Sata150, Sata300, Sata600};
use anyhow::{anyhow, bail, Context, Error, Result};
use process_data::pci_slot::PciSlot;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Default)]
pub enum Link {
    Pcie(LinkData<PcieLinkData>),
    Sata(LinkData<SataSpeed>),
    #[default]
    Unknown,
}

#[derive(Debug)]
pub struct LinkData<T> {
    pub current: Result<T>,
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
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SataSpeed {
    Sata150,
    Sata300,
    Sata600,
}

impl LinkData<PcieLinkData> {
    pub fn from_pci_slot(pci_slot: &PciSlot) -> Result<Self> {
        let pcie_dir = format!("/sys/bus/pci/devices/{pci_slot}/");
        let pcie_folder = Path::new(pcie_dir.as_str());
        if pcie_folder.exists() {
            return Self::read_pcie_link_data(&pcie_folder.to_path_buf());
        }
        bail!("Could not find PCIe address entry for {pci_slot}");
    }

    fn read_pcie_link_data(path: &PathBuf) -> Result<Self> {
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

        let current = PcieLinkData::parse(&current_pcie_speed_raw, &current_pcie_width_raw);
        let max = if let (Ok(speed), Ok(width)) = (max_pcie_speed_raw, max_pcie_width_raw) {
            PcieLinkData::parse(&speed, &width)
        } else {
            Err(anyhow!("Could not parse max PCIe link"))
        };
        Ok(Self { current, max })
    }
}

impl PcieLinkData {
    pub fn parse(speed_raw: &str, width_raw: &str) -> Result<Self> {
        let speed = PcieSpeed::from_str(speed_raw)?;
        let width = width_raw
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
        if let Ok(current) = self.current {
            let has_different_max = {
                if let Ok(max) = self.max {
                    current != max
                } else {
                    false
                }
            };
            if has_different_max {
                write!(f, "{} / {}", current, self.max.as_ref().unwrap())
            } else {
                write!(f, "{}", current)
            }
        } else {
            write!(f, "{}", i18n("N/A"))
        }
    }
}

impl LinkData<SataSpeed> {
    pub fn from_ata_slot(ata_slot: &AtaSlot) -> Result<Self> {
        let ata_link_path =
            Path::new("/sys/class/ata_link").join(format!("link{}", ata_slot.ata_link));
        if std::fs::exists(&ata_link_path)? {
            let current_sata_speed_raw = std::fs::read_to_string(ata_link_path.join("sata_spd"))
                .map(|x| x.trim().to_string())
                .context("Could not read sata_spd")?;
            let max_sata_speed_raw = std::fs::read_to_string(ata_link_path.join("sata_spd_max"))
                .map(|x| x.trim().to_string())
                .context("Could not read sata_spd_max");

            let current = SataSpeed::from_str(&current_sata_speed_raw);
            let max = if let Ok(max_sata_speed_raw) = max_sata_speed_raw {
                SataSpeed::from_str(&max_sata_speed_raw)
            } else {
                Err(anyhow::anyhow!("Could not read sata_spd_max"))
            };
            return Ok(Self { current, max });
        }
        bail!("ata link path not found for '{:?}'", ata_slot);
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
impl Display for Link {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Link::Pcie(data) => data.to_string(),
                Link::Sata(data) => data.to_string(),
                Link::Unknown => i18n("N/A"),
            }
        )
    }
}

#[cfg(test)]
mod test {
    use crate::utils::link::{LinkData, PcieLinkData, PcieSpeed, SataSpeed};
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
        ]);

        for input in map.keys() {
            let result = PcieSpeed::from_str(input);
            assert!(result.is_ok(), "Could not parse PCIe speed for '{}'", input);
            let expected = map[input];
            pretty_assertions::assert_eq!(expected, result.unwrap());
        }
    }

    #[test]
    fn parse_pcie_link_speeds_failure() {
        let invalid = vec!["128.0 GT/s PCIe", "SOMETHING_ELSE", ""];

        for input in invalid {
            let result = PcieSpeed::from_str(input);
            assert!(
                result.is_err(),
                "Could parse PCIe speed for '{}' while we don't expect that",
                input
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
                current: Ok(link_data.clone()),
                max: Ok(link_data.clone()),
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
                current: Ok(link_data.clone()),
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
                        current: Ok(current_data.clone()),
                        max: Ok(max_data.clone()),
                    };
                    let result = input.to_string();
                    let expected =
                        format!("{} / {}", current_data.to_string(), max_data.to_string());
                    pretty_assertions::assert_str_eq!(expected, result);
                }
            }
        }
    }

    #[test]
    fn display_pcie_link_different_max_2() {
        let input = LinkData {
            current: Ok(PcieLinkData {
                speed: PcieSpeed::Pcie40,
                width: 8,
            }),
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
            assert!(result.is_ok(), "Could not parse SATA speed for '{}'", input);
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
                "Could parse SATA speed for '{}' while we don't expect that",
                input
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
}
