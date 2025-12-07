use std::{
    fmt::{self, Display},
    num::ParseIntError,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PciSlotError {
    #[error("PCI slot number out of bounds (is {0}, must be < 32)")]
    NumberOutOfBounds(u8),

    #[error("PCI slot function out of bounds (is {0}, must be < 8)")]
    FunctionOutOfBounds(u8),

    #[error("invalid PCI slot format: expected 'domain:bus:number.function'")]
    InvalidFormat,

    #[error(transparent)]
    ParseDomain(ParseIntError),

    #[error(transparent)]
    ParseBus(ParseIntError),

    #[error(transparent)]
    ParseNumber(ParseIntError),

    #[error(transparent)]
    ParseFunction(ParseIntError),
}

/// Represents a PCI slot identifier.
///
/// A PCI slot is uniquely identified by its domain (16 bits), bus (8 bits), number (5 bits), and function (3 bits) fields.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Default, Hash, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct PciSlot {
    data: u32,
}

impl PciSlot {
    /// Constructs a new `PciSlot` with the specified domain, bus, number, and function.
    ///
    /// # Example
    /// ```
    /// use process_data::pci_slot::PciSlot;
    ///
    /// let pci_slot = PciSlot::try_new(0x0, 0x1, 0x1b, 0x3).unwrap();
    /// assert_eq!(pci_slot.domain(), 0x0);
    /// assert_eq!(pci_slot.bus(), 0x1);
    /// assert_eq!(pci_slot.number(), 0x1b);
    /// assert_eq!(pci_slot.function(), 0x3);
    /// ```
    ///
    /// # Errors
    /// Returns a `PciSlotError` error if any of the input values are out of bounds.
    pub fn try_new(domain: u16, bus: u8, number: u8, function: u8) -> Result<Self, PciSlotError> {
        if number > 0x1f {
            return Err(PciSlotError::NumberOutOfBounds(number));
        }

        if function > 0x7 {
            return Err(PciSlotError::FunctionOutOfBounds(function));
        }

        Ok(PciSlot {
            data: (u32::from(domain) << 16)
                | (u32::from(bus) << 8)
                | (u32::from(number) << 3)
                | (u32::from(function)),
        })
    }

    /// Returns the domain of the PCI slot.
    pub fn domain(&self) -> u16 {
        ((self.data >> 16) & 0xffff) as u16
    }

    /// Returns the bus of the PCI slot.
    pub fn bus(&self) -> u8 {
        ((self.data >> 8) & 0xff) as u8
    }

    /// Returns the number of the PCI slot. This is always a 5-bit value.
    pub fn number(&self) -> u8 {
        ((self.data >> 3) & 0x1f) as u8
    }

    /// Returns the function of the PCI slot. This is always a 3-bit value.
    pub fn function(&self) -> u8 {
        (self.data & 0x7) as u8
    }
}

impl FromStr for PciSlot {
    type Err = PciSlotError;

    /// Parses a string into a `PciSlot`.
    ///
    /// The input string should be in the format "domain:bus:number.function".
    /// Each component (domain, bus, number, function) must be a valid hexadecimal value.
    ///
    /// # Example
    /// ```
    /// use process_data::pci_slot::PciSlot;
    /// use std::str::FromStr;
    ///
    /// let pci_slot = PciSlot::from_str("0000:01:1b.3").unwrap();
    /// assert_eq!(pci_slot.domain(), 0x0);
    /// assert_eq!(pci_slot.bus(), 0x1);
    /// assert_eq!(pci_slot.number(), 0x1b);
    /// assert_eq!(pci_slot.function(), 0x3);
    /// ```
    ///
    /// # Errors
    /// Returns a `PciSlotError` if the input string could not be parsed or if there are out of bounds values.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Split the input string by ':' and '.' to extract the domain, bus, number, and function
        const PCI_SLOT_SEPARATORS: &[char] = &[':', '.'];
        let parts: Vec<&str> = s.split(|c| PCI_SLOT_SEPARATORS.contains(&c)).collect();

        if parts.len() != 4 {
            return Err(PciSlotError::InvalidFormat);
        }

        // Parse each component as a hexadecimal value
        let domain = u16::from_str_radix(parts[0], 16).map_err(PciSlotError::ParseDomain)?;
        let bus = u8::from_str_radix(parts[1], 16).map_err(PciSlotError::ParseBus)?;
        let number = u8::from_str_radix(parts[2], 16).map_err(PciSlotError::ParseNumber)?;
        let function = u8::from_str_radix(parts[3], 16).map_err(PciSlotError::ParseFunction)?;

        PciSlot::try_new(domain, bus, number, function)
    }
}

impl Display for PciSlot {
    /// Formats the `PciSlot` as a string in the format "domain:bus:number.function".
    ///
    /// # Example
    /// ```
    /// use process_data::pci_slot::PciSlot;
    ///
    /// let pci_slot = PciSlot::try_new(0x0, 0x1, 0x1b, 0x3).unwrap();
    /// assert_eq!(pci_slot.to_string(), "0000:01:1b.3");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04x}:{:02x}:{:02x}.{:x}",
            self.domain(),
            self.bus(),
            self.number(),
            self.function(),
        )
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::{PciSlot, PciSlotError};
    use pretty_assertions::assert_eq;

    #[test]
    fn pci_id_from_string() {
        let pci_id = PciSlot::try_new(0x5, 0x1, 0x1b, 0x3).unwrap();
        let pci_id_str = "0005:01:1b.3";
        assert_eq!(pci_id, PciSlot::from_str(pci_id_str).unwrap());
    }

    #[test]
    fn pci_id_to_string() {
        let pci_id = PciSlot::try_new(0x5, 0x1, 0x1b, 0x3).unwrap();
        let pci_id_str = "0005:01:1b.3";
        assert_eq!(pci_id_str, pci_id.to_string());
    }

    #[test]
    fn invalid_format_too_few_parts() {
        let pci_id_str = "0000:01:1b";
        assert_eq!(
            PciSlot::from_str(pci_id_str).unwrap_err(),
            PciSlotError::InvalidFormat
        );
    }

    #[test]
    fn invalid_format_too_many_parts() {
        let pci_id_str = "0000:01:1b.3.4";
        assert_eq!(
            PciSlot::from_str(pci_id_str).unwrap_err(),
            PciSlotError::InvalidFormat
        );
    }

    #[test]
    fn invalid_domain_hex_value() {
        let pci_id_str = "g000:01:1b.3";
        assert!(matches!(
            PciSlot::from_str(pci_id_str).unwrap_err(),
            PciSlotError::ParseDomain(_)
        ));
    }

    #[test]
    fn invalid_bus_hex_value() {
        let pci_id_str = "0000:z1:1b.3";
        assert!(matches!(
            PciSlot::from_str(pci_id_str).unwrap_err(),
            PciSlotError::ParseBus(_)
        ));
    }

    #[test]
    fn invalid_number_hex_value() {
        let pci_id_str = "0000:01:x2.3";
        assert!(matches!(
            PciSlot::from_str(pci_id_str).unwrap_err(),
            PciSlotError::ParseNumber(_)
        ));
    }

    #[test]
    fn invalid_function_hex_value() {
        let pci_id_str = "0000:01:1b.y";
        assert!(matches!(
            PciSlot::from_str(pci_id_str).unwrap_err(),
            PciSlotError::ParseFunction(_)
        ));
    }

    #[test]
    fn minimal_valid_values() {
        let pci_id = PciSlot::try_new(0x0, 0x0, 0x0, 0x0).unwrap();
        let pci_id_str = "0000:00:00.0";
        assert_eq!(pci_id, PciSlot::from_str(pci_id_str).unwrap());
    }

    #[test]
    fn maximal_valid_values() {
        let pci_id = PciSlot::try_new(0xffff, 0xff, 0x1f, 0x7).unwrap();
        let pci_id_str = "ffff:ff:1f.7";
        assert_eq!(pci_id, PciSlot::from_str(pci_id_str).unwrap());
    }

    #[test]
    fn number_too_high() {
        let result = PciSlot::try_new(0x0, 0x0, 0x20, 0x0);
        assert!(matches!(result, Err(PciSlotError::NumberOutOfBounds(0x20))));
    }

    #[test]
    fn function_too_high() {
        let result = PciSlot::try_new(0x0, 0x0, 0x0, 0x8);
        assert!(matches!(
            result,
            Err(PciSlotError::FunctionOutOfBounds(0x8))
        ));
    }
}
