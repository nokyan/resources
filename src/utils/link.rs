use crate::i18n::i18n;
use crate::utils::drive::{Drive, DriveType};
use crate::utils::gpu::{Gpu, GpuImpl};
use anyhow::{anyhow, bail, Context, Error, Result};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Default)]
pub enum Link {
    Pcie(PcieLink),
    #[default]
    Unknown,
}

#[derive(Debug)]
pub struct PcieLink {
    pub current: Result<PcieLinkData>,
    pub max: Result<PcieLinkData>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PcieLinkData {
    pub speed: PcieSpeed,
    pub width: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PcieSpeed {
    Pcie10,
    Pcie20,
    Pcie30,
    Pcie40,
    Pcie50,
    Pcie60,
    #[default]
    Unknown,
}
impl Link {
    pub fn for_drive(drive: &Drive) -> Result<Self> {
        //For now only PCIe, later SATA
        match PcieLink::for_drive(drive) {
            Ok(link) => Ok(Link::Pcie(link)),
            Err(e) => Err(e),
        }
    }

    pub fn for_gpu(gpu: &Gpu) -> Result<Self> {
        match PcieLink::for_gpu(gpu) {
            Ok(link) => Ok(Link::Pcie(link)),
            Err(e) => Err(e),
        }
    }
}
impl PcieLink {
    pub fn for_drive(drive: &Drive) -> Result<Self> {
        match drive.drive_type {
            DriveType::Nvme => Self::for_nvme(&drive.sysfs_path),
            _ => Err(anyhow!("Unsupported drive type")),
        }
    }

    pub fn for_gpu(gpu: &Gpu) -> Result<Self> {
        let drm_path = match gpu {
            Gpu::Amd(data) => data.sysfs_path(),
            Gpu::Intel(data) => data.sysfs_path(),
            Gpu::Nvidia(data) => data.sysfs_path(),
            Gpu::V3d(data) => data.sysfs_path(),
            Gpu::Other(data) => data.sysfs_path(),
        };
        let device_path = drm_path.join("device");
        Self::read_pcie_link_data(&device_path.to_path_buf())
    }
    fn for_nvme(path: &PathBuf) -> Result<Self> {
        let address_path = path.join("device").join("address");
        let address = std::fs::read_to_string(address_path)
            .map(|x| x.trim().to_string())
            .context("Failed to read nvme PCIe address");
        if let Ok(address) = address {
            Self::read_using_pcie_address(&address)
        } else {
            bail!("Could not find PCIe address in sysfs for nvme")
        }
    }

    fn read_using_pcie_address(pcie_address: &str) -> Result<Self> {
        let pcie_dir = format!("/sys/bus/pci/devices/{pcie_address}/");
        let pcie_folder = Path::new(pcie_dir.as_str());
        if pcie_folder.exists() {
            return Self::read_pcie_link_data(&pcie_folder.to_path_buf());
        }
        bail!("Could not find PCIe speed")
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
        Ok(PcieLink { current, max })
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

impl Display for PcieLink {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Ok(current) = self.current {
            let different_max = {
                if let Ok(max) = self.max {
                    current != max
                } else {
                    false
                }
            };
            if different_max {
                write!(f, "{} / {}", current, self.max.as_ref().unwrap())
            } else {
                write!(f, "{}", current)
            }
        } else {
            write!(f, "{}", i18n("N/A"))
        }
    }
}

impl Display for PcieLinkData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} x{}", self.speed, self.width)
    }
}

impl Display for PcieSpeed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PcieSpeed::Pcie10 => "PCIe 1.0".to_string(),
                PcieSpeed::Pcie20 => "PCIe 2.0".to_string(),
                PcieSpeed::Pcie30 => "PCIe 3.0".to_string(),
                PcieSpeed::Pcie40 => "PCIe 4.0".to_string(),
                PcieSpeed::Pcie50 => "PCIe 5.0".to_string(),
                PcieSpeed::Pcie60 => "PCIe 6.0".to_string(),
                PcieSpeed::Unknown => i18n("N/A"),
            }
        )
    }
}
impl FromStr for PcieSpeed {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "2.5 GT/s PCIe" => Ok(PcieSpeed::Pcie10),
            "5.0 GT/s PCIe" => Ok(PcieSpeed::Pcie20),
            "8.0 GT/s PCIe" => Ok(PcieSpeed::Pcie30),
            "16.0 GT/s PCIe" => Ok(PcieSpeed::Pcie40),
            "32.0 GT/s PCIe" => Ok(PcieSpeed::Pcie50),
            "64.0 GT/s PCIe" => Ok(PcieSpeed::Pcie60),
            _ => Err(Error::msg("Could not parse PCIe speed")),
        }
    }
}

impl Display for Link {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Link::Pcie(data) => data.to_string(),
                Link::Unknown => i18n("N/A"),
            }
        )
    }
}

#[cfg(test)]
mod test {
    use crate::utils::link::{PcieLink, PcieLinkData, PcieSpeed};
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
    fn display_pcie_link_identical_current_max_only_once() {
        let map = get_test_pcie_link_data();

        for link_data in map.keys() {
            let input = PcieLink {
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
            let input = PcieLink {
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
                    let input = PcieLink {
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
        let input = PcieLink {
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
        let expected = "PCIe 4.0 x8 / PCIe 5.0 x16";
        pretty_assertions::assert_str_eq!(expected, result);
    }

    fn get_test_pcie_link_data() -> HashMap<PcieLinkData, &'static str> {
        HashMap::from([
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie10,
                    width: 2,
                },
                "PCIe 1.0 x2",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie20,
                    width: 4,
                },
                "PCIe 2.0 x4",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie30,
                    width: 1,
                },
                "PCIe 3.0 x1",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie40,
                    width: 8,
                },
                "PCIe 4.0 x8",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie50,
                    width: 16,
                },
                "PCIe 5.0 x16",
            ),
            (
                PcieLinkData {
                    speed: PcieSpeed::Pcie60,
                    width: 1,
                },
                "PCIe 6.0 x1",
            ),
        ])
    }
}
