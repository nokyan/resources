use std::{error::Error, fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Default, Hash, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct PciSlot {
    pub domain: u16,
    pub bus: u8,
    pub number: u8,
    pub function: u8,
}

impl PciSlot {
    pub fn new(domain: u16, bus: u8, number: u8, function: u8) -> Self {
        Self {
            domain,
            bus,
            number,
            function,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ParseError(String);

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }

    fn description(&self) -> &str {
        "description() is deprecated; use Display"
    }

    fn cause(&self) -> Option<&dyn Error> {
        self.source()
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unable to parse to PCI ID")
    }
}

impl FromStr for PciSlot {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let dot_split: Vec<&str> = s.split('.').collect();

        if dot_split.len() != 2 {
            return Err(ParseError("amount of '.' ≠ 1".into()));
        }

        let colon_split: Vec<&str> = dot_split[0].split(':').collect();

        if colon_split.len() != 3 {
            return Err(ParseError("amount of ':' ≠ 2".into()));
        }

        let domain = u16::from_str_radix(colon_split[0], 16)
            .or(Err(ParseError("unable to parse domain".into())))?;

        let bus = u8::from_str_radix(colon_split[1], 16)
            .or(Err(ParseError("unable to parse bus".into())))?;

        let number = u8::from_str_radix(colon_split[2], 16)
            .or(Err(ParseError("unable to parse number".into())))?;

        let function = u8::from_str_radix(dot_split[1], 16)
            .or(Err(ParseError("unable to parse function".into())))?;

        Ok(PciSlot {
            domain,
            bus,
            number,
            function,
        })
    }
}

impl Display for PciSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04x}:{:02x}:{:02x}.{:x}",
            self.domain, self.bus, self.number, self.function,
        )
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::PciSlot;

    #[test]
    fn pci_id_from_string() {
        let pci_id = PciSlot::new(0x0, 0x1, 0xfe, 0x3);
        let pci_id_str = "0000:01:fe.3";
        assert_eq!(pci_id, PciSlot::from_str(pci_id_str).unwrap());
    }

    #[test]
    fn pci_id_to_string() {
        let pci_id = PciSlot::new(0x0, 0x1, 0xfe, 0x3);
        let pci_id_str = "0000:01:fe.3";
        assert_eq!(pci_id_str, pci_id.to_string());
    }
}
