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

/// A PCI slot identifier packed into a single `u32`.
///
/// Layout: `[domain(16) | bus(8) | number(5) | function(3)]`
#[derive(
    Debug, Clone, Copy, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct PciSlot {
    data: u32,
}

impl PciSlot {
    /// Construct a [`PciSlot`], validating that `number < 32` and `function < 8`.
    ///
    /// ```
    /// use process_data::pci_slot::PciSlot;
    ///
    /// let slot = PciSlot::try_new(0x0, 0x1, 0x1b, 0x3).unwrap();
    /// assert_eq!((slot.domain(), slot.bus(), slot.number(), slot.function()),
    ///           (0x0, 0x1, 0x1b, 0x3));
    /// ```
    pub fn try_new(domain: u16, bus: u8, number: u8, function: u8) -> Result<Self, PciSlotError> {
        if number > 0x1f {
            return Err(PciSlotError::NumberOutOfBounds(number));
        }
        if function > 0x7 {
            return Err(PciSlotError::FunctionOutOfBounds(function));
        }
        Ok(Self {
            data: (u32::from(domain) << 16)
                | (u32::from(bus) << 8)
                | (u32::from(number) << 3)
                | u32::from(function),
        })
    }

    pub fn domain(self) -> u16 {
        ((self.data >> 16) & 0xffff) as u16
    }
    pub fn bus(self) -> u8 {
        ((self.data >> 8) & 0xff) as u8
    }
    pub fn number(self) -> u8 {
        ((self.data >> 3) & 0x1f) as u8
    }
    pub fn function(self) -> u8 {
        (self.data & 0x7) as u8
    }
}

impl FromStr for PciSlot {
    type Err = PciSlotError;

    /// Parse `"domain:bus:number.function"` (all components are hexadecimal).
    ///
    /// ```
    /// use process_data::pci_slot::PciSlot;
    /// let slot: PciSlot = "0000:01:1b.3".parse().unwrap();
    /// assert_eq!(slot.to_string(), "0000:01:1b.3");
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split([':', '.']).collect();
        if parts.len() != 4 {
            return Err(PciSlotError::InvalidFormat);
        }
        let domain = u16::from_str_radix(parts[0], 16).map_err(PciSlotError::ParseDomain)?;
        let bus = u8::from_str_radix(parts[1], 16).map_err(PciSlotError::ParseBus)?;
        let number = u8::from_str_radix(parts[2], 16).map_err(PciSlotError::ParseNumber)?;
        let function = u8::from_str_radix(parts[3], 16).map_err(PciSlotError::ParseFunction)?;
        Self::try_new(domain, bus, number, function)
    }
}

impl Display for PciSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04x}:{:02x}:{:02x}.{:x}",
            self.domain(),
            self.bus(),
            self.number(),
            self.function()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn round_trip_from_str() {
        let slot = PciSlot::try_new(0x5, 0x1, 0x1b, 0x3).unwrap();
        assert_eq!(slot, "0005:01:1b.3".parse().unwrap());
    }

    #[test]
    fn round_trip_to_str() {
        assert_eq!(
            PciSlot::try_new(0x5, 0x1, 0x1b, 0x3).unwrap().to_string(),
            "0005:01:1b.3"
        );
    }

    #[test]
    fn invalid_format_too_few_parts() {
        assert_eq!(
            "0000:01:1b".parse::<PciSlot>().unwrap_err(),
            PciSlotError::InvalidFormat
        );
    }

    #[test]
    fn invalid_format_too_many_parts() {
        assert_eq!(
            "0000:01:1b.3.4".parse::<PciSlot>().unwrap_err(),
            PciSlotError::InvalidFormat
        );
    }

    #[test]
    fn invalid_domain() {
        assert!(matches!(
            "g000:01:1b.3".parse::<PciSlot>().unwrap_err(),
            PciSlotError::ParseDomain(_)
        ));
    }

    #[test]
    fn invalid_bus() {
        assert!(matches!(
            "0000:z1:1b.3".parse::<PciSlot>().unwrap_err(),
            PciSlotError::ParseBus(_)
        ));
    }

    #[test]
    fn invalid_number() {
        assert!(matches!(
            "0000:01:x2.3".parse::<PciSlot>().unwrap_err(),
            PciSlotError::ParseNumber(_)
        ));
    }

    #[test]
    fn invalid_function() {
        assert!(matches!(
            "0000:01:1b.y".parse::<PciSlot>().unwrap_err(),
            PciSlotError::ParseFunction(_)
        ));
    }

    #[test]
    fn minimal_valid() {
        assert_eq!(
            PciSlot::try_new(0, 0, 0, 0).unwrap().to_string(),
            "0000:00:00.0"
        );
    }

    #[test]
    fn maximal_valid() {
        let slot = PciSlot::try_new(0xffff, 0xff, 0x1f, 0x7).unwrap();
        assert_eq!(slot, "ffff:ff:1f.7".parse().unwrap());
    }

    #[test]
    fn number_out_of_bounds() {
        assert!(matches!(
            PciSlot::try_new(0, 0, 0x20, 0),
            Err(PciSlotError::NumberOutOfBounds(0x20))
        ));
    }

    #[test]
    fn function_out_of_bounds() {
        assert!(matches!(
            PciSlot::try_new(0, 0, 0, 0x8),
            Err(PciSlotError::FunctionOutOfBounds(0x8))
        ));
    }
}
