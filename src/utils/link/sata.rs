use crate::utils::drive::AtaSlot;
use crate::utils::link::LinkData;
use crate::utils::link::sata::SataSpeed::{Sata150, Sata300, Sata600};
use anyhow::{Context, Error, anyhow};
use log::trace;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SataSpeed {
    Sata150,
    Sata300,
    Sata600,
}

impl LinkData<SataSpeed> {
    pub fn from_ata_slot(ata_slot: &AtaSlot) -> anyhow::Result<Self> {
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

#[cfg(test)]
mod test {
    use crate::utils::link::sata::SataSpeed;
    use std::collections::HashMap;
    use std::str::FromStr;

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
}
