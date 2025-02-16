use crate::i18n::i18n;
use anyhow::Result;
use std::fmt::{Display, Formatter};

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
    #[default]
    Unknown,
}

impl Display for PcieLink {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Ok(current) = self.current {
            let different_max = {
                if let Ok(max) = self.max {
                    current == max
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
        write!(f, "{} {}x", self.speed, self.width)
    }
}
impl Display for PcieSpeed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PcieSpeed::Pcie10 => "PCIE 1.0".to_string(),
                PcieSpeed::Pcie20 => "PCIE 2.0".to_string(),
                PcieSpeed::Pcie30 => "PCIE 3.0".to_string(),
                PcieSpeed::Pcie40 => "PCIE 4.0".to_string(),
                PcieSpeed::Pcie50 => "PCIE 5.0".to_string(),
                PcieSpeed::Unknown => i18n("N/A"),
            }
        )
    }
}
