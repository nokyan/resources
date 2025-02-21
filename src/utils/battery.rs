use std::{
    fmt::Display,
    path::{Path, PathBuf},
    str::{self, FromStr},
};

use crate::i18n::{i18n, i18n_f};
use anyhow::{Context, Result, bail};
use lazy_regex::{Lazy, Regex, lazy_regex};
use log::trace;

use super::units::convert_energy;

// For (at least) Lenovo Yoga 6 13ALC7
static HEX_ENCODED_REGEX: Lazy<Regex> = lazy_regex!(r"^(0x[0-9a-fA-F]{2}\s*)*$");

#[derive(Debug)]
pub struct BatteryData {
    pub inner: Battery,
    pub charge: Result<f64>,
    pub power_usage: Result<f64>,
    pub health: Result<f64>,
    pub state: Result<State>,
    pub charge_cycles: Result<usize>,
}

impl BatteryData {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();

        trace!("Gathering battery data for {path:?}…");

        let inner = Battery::from_sysfs(path);
        let charge = inner.charge();
        let power_usage = inner.power_usage();
        let health = inner.health();
        let state = inner.state();
        let charge_cycles = inner.charge_cycles();

        let battery_data = Self {
            inner,
            charge,
            power_usage,
            health,
            state,
            charge_cycles,
        };

        trace!(
            "Gathered battery data for {}: {battery_data:?}",
            path.to_string_lossy()
        );

        battery_data
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub enum State {
    Charging,
    Discharging,
    Empty,
    Full,
    #[default]
    Unknown,
}

impl str::FromStr for State {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let state = match s.to_ascii_lowercase().as_str() {
            "charging" => State::Charging,
            "discharging" => State::Discharging,
            "empty" => State::Empty,
            "full" => State::Full,
            _ => State::Unknown,
        };

        Ok(state)
    }
}

impl Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                State::Charging => i18n("Charging"),
                State::Discharging => i18n("Discharging"),
                State::Empty => i18n("Empty"),
                State::Full => i18n("Full"),
                State::Unknown => i18n("Unknown"),
            }
        )
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub enum Technology {
    NickelMetalHydride,
    NickelCadmium,
    NickelZinc,
    LeadAcid,
    LithiumIon,
    LithiumIronPhosphate,
    LithiumPolymer,
    RechargeableAlkalineManganese,
    #[default]
    Unknown,
}

impl str::FromStr for Technology {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tech = match s.to_ascii_lowercase().as_str() {
            "nimh" => Technology::NickelMetalHydride,
            "nicd" => Technology::NickelCadmium,
            "nizn" => Technology::NickelZinc,
            "pb" | "pbac" => Technology::LeadAcid,
            "li-i" | "li-ion" | "lion" => Technology::LithiumIon,
            "life" => Technology::LithiumIronPhosphate,
            "lip" | "lipo" | "li-poly" => Technology::LithiumPolymer,
            "ram" => Technology::RechargeableAlkalineManganese,
            _ => Technology::Unknown,
        };

        Ok(tech)
    }
}

impl Display for Technology {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Technology::NickelMetalHydride => i18n("Nickel-Metal Hydride"),
                Technology::NickelCadmium => i18n("Nickel-Cadmium"),
                Technology::NickelZinc => i18n("Nickel-Zinc"),
                Technology::LeadAcid => i18n("Lead-Acid"),
                Technology::LithiumIon => i18n("Lithium-Ion"),
                Technology::LithiumIronPhosphate => i18n("Lithium Iron Phosphate"),
                Technology::LithiumPolymer => i18n("Lithium Polymer"),
                Technology::RechargeableAlkalineManganese =>
                    i18n("Rechargeable Alkaline Managanese"),
                Technology::Unknown => i18n("N/A"),
            }
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Battery {
    pub sysfs_path: PathBuf,
    pub manufacturer: Option<String>,
    pub model_name: Option<String>,
    pub design_capacity: Option<f64>,
    pub technology: Technology,
}

impl Battery {
    pub fn get_sysfs_paths() -> Result<Vec<PathBuf>> {
        let mut list = Vec::new();
        let entries = std::fs::read_dir("/sys/class/power_supply")?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if Self::is_valid_power_supply(&path) {
                list.push(path);
            }
        }
        Ok(list)
    }

    pub fn is_valid_power_supply<P: AsRef<Path>>(path: P) -> bool {
        let path = path.as_ref();

        //type == "Battery"
        //scope != "Device" (HID device batteries)

        let power_supply_type_is_battery = std::fs::read_to_string(path.join("type"))
            .is_ok_and(|ps_type| ps_type.trim() == "Battery");

        let power_supply_scope_is_not_device = std::fs::read_to_string(path.join("scope"))
            .map(|ps_scope| ps_scope.trim() != "Device")
            .unwrap_or(true);

        power_supply_type_is_battery && power_supply_scope_is_not_device
    }

    pub fn from_sysfs<P: AsRef<Path>>(sysfs_path: P) -> Battery {
        let sysfs_path = sysfs_path.as_ref().to_path_buf();

        trace!("Creating Battery object of {sysfs_path:?}…");

        let manufacturer = std::fs::read_to_string(sysfs_path.join("manufacturer"))
            .map(|s| Self::untangle_weird_encoding(s.replace('\n', "")))
            .ok();

        let model_name = std::fs::read_to_string(sysfs_path.join("model_name"))
            .map(|s| Self::untangle_weird_encoding(s.replace('\n', "")))
            .ok();

        let technology = Technology::from_str(
            &std::fs::read_to_string(sysfs_path.join("technology"))
                .map(|s| s.replace('\n', ""))
                .unwrap_or_default(),
        )
        .unwrap_or_default();

        let design_capacity = std::fs::read_to_string(sysfs_path.join("energy_full_design"))
            .context("unable to find any energy_full_design")
            .and_then(|capacity| {
                capacity
                    .trim()
                    .parse::<usize>()
                    .map(|int| int as f64 / 1_000_000.0)
                    .context("can't parse energy_full_design")
            })
            .ok();

        let battery = Battery {
            sysfs_path: sysfs_path.clone(),
            manufacturer,
            model_name,
            design_capacity,
            technology,
        };

        trace!("Created Battery object of {sysfs_path:?}: {battery:?}");

        battery
    }

    // apparently some manufacturers like to for whatever reason reencode the manufacturer and model name in hex or
    // similar, this function will try to untangle it
    fn untangle_weird_encoding<S: AsRef<str>>(s: S) -> String {
        if HEX_ENCODED_REGEX.is_match(s.as_ref()) {
            String::from_utf8_lossy(
                &s.as_ref()
                    .split_whitespace()
                    .flat_map(|hex| u8::from_str_radix(&hex.replace("0x", ""), 16))
                    .map(|byte| if byte == 0x0 { b' ' } else { byte }) // gtk will crash when encountering NUL
                    .collect::<Vec<u8>>(),
            )
            .to_string()
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn display_name(&self) -> String {
        if let Some(design_capacity) = self.design_capacity {
            let converted_energy = convert_energy(design_capacity, true);
            i18n_f("{} Battery", &[&converted_energy])
        } else {
            i18n("Battery")
        }
    }

    pub fn charge(&self) -> Result<f64> {
        std::fs::read_to_string(self.sysfs_path.join("capacity"))
            .context("unable to read capacity sysfs file")
            .and_then(|capacity| {
                capacity
                    .trim()
                    .parse::<u8>()
                    .map(|percent| f64::from(percent) / 100.0)
                    .context("unable to parse capacity sysfs file")
            })
            .or_else(|_| self.charge_from_energy())
    }

    pub fn charge_from_energy(&self) -> Result<f64> {
        let energy_now = std::fs::read_to_string(self.sysfs_path.join("energy_now"))
            .context("unable to read energy_now sysfs file")
            .and_then(|x| {
                x.trim()
                    .parse::<usize>()
                    .context("unable to parse energy_now sysfs file")
            });

        let energy_full = std::fs::read_to_string(self.sysfs_path.join("energy_full"))
            .context("unable to read energy_full sysfs file")
            .and_then(|x| {
                x.trim()
                    .parse::<usize>()
                    .context("unable to parse energy_full sysfs file")
            });

        if let (Ok(energy_now), Ok(energy_full)) = (energy_now, energy_full) {
            Ok(energy_now as f64 / energy_full as f64)
        } else {
            bail!("no charge from energy information found");
        }
    }

    pub fn health(&self) -> Result<f64> {
        let energy_full = std::fs::read_to_string(self.sysfs_path.join("energy_full"))
            .context("unable to read energy_full sysfs file")
            .and_then(|x| {
                x.trim()
                    .parse::<usize>()
                    .context("unable to parse energy_full sysfs file")
            });

        let energy_full_design =
            std::fs::read_to_string(self.sysfs_path.join("energy_full_design"))
                .context("unable to read energy_full_design sysfs file")
                .and_then(|x| {
                    x.trim()
                        .parse::<usize>()
                        .context("unable to parse energy_full_design sysfs file")
                });

        if let (Ok(energy_full), Ok(energy_full_design)) = (energy_full, energy_full_design) {
            Ok(energy_full as f64 / energy_full_design as f64)
        } else {
            let charge_full = std::fs::read_to_string(self.sysfs_path.join("charge_full"))
                .context("unable to read charge_full sysfs file")
                .and_then(|x| {
                    x.trim()
                        .parse::<usize>()
                        .context("unable to parse charge_full sysfs file")
                });

            let charge_full_design =
                std::fs::read_to_string(self.sysfs_path.join("charge_full_design"))
                    .context("unable to read charge_full_design sysfs file")
                    .and_then(|x| {
                        x.trim()
                            .parse::<usize>()
                            .context("unable to parse charge_full_design sysfs file")
                    });

            if let (Ok(charge_full), Ok(charge_full_design)) = (charge_full, charge_full_design) {
                Ok(charge_full as f64 / charge_full_design as f64)
            } else {
                bail!("no health information found")
            }
        }
    }

    pub fn power_usage(&self) -> Result<f64> {
        std::fs::read_to_string(self.sysfs_path.join("power_now"))
            .context("unable to read power_now file")
            .and_then(|x| {
                x.trim()
                    .parse::<isize>()
                    .map(|microwatts| microwatts.abs() as f64 / 1_000_000.0)
                    .context("unable to parse power_now sysfs file")
            })
            .or_else(|_| self.power_usage_from_voltage_and_current())
    }

    fn power_usage_from_voltage_and_current(&self) -> Result<f64> {
        let voltage = std::fs::read_to_string(self.sysfs_path.join("voltage_now"))?
            .trim()
            .parse::<usize>()
            .map(|microvolts| microvolts as f64 / 1_000_000.0)
            .context("unable to parse voltage_now sysfs file")?;

        let current = std::fs::read_to_string(self.sysfs_path.join("current_now"))?
            .trim()
            .parse::<usize>()
            .map(|microamps| microamps as f64 / 1_000_000.0)
            .context("unable to parse current_now sysfs file")?;

        Ok(f64::abs(voltage * current))
    }

    pub fn state(&self) -> Result<State> {
        State::from_str(
            &std::fs::read_to_string(self.sysfs_path.join("status"))
                .map(|s| s.replace('\n', ""))?,
        )
    }

    pub fn charge_cycles(&self) -> Result<usize> {
        std::fs::read_to_string(self.sysfs_path.join("cycle_count"))?
            .trim()
            .parse()
            .context("unable to parse cycle_count sysfs file")
    }
}

#[cfg(test)]
mod test {
    use super::Battery;
    use pretty_assertions::assert_eq;

    #[test]
    fn dont_untangle_untangled_string() {
        let untangled_string = String::from("This is a normal string");
        assert_eq!(
            Battery::untangle_weird_encoding(untangled_string),
            String::from("This is a normal string")
        )
    }

    #[test]
    fn dont_untangle_non_hex_bytes() {
        let non_hex_bytes = String::from("0xxy 0x4g");
        assert_eq!(
            Battery::untangle_weird_encoding(non_hex_bytes),
            String::from("0xxy 0x4g")
        )
    }

    #[test]
    fn untangle_tangled_string() {
        let tangled_string = String::from("0x41 0x42 0x43  0x58 0x59 0x5A");
        assert_eq!(
            Battery::untangle_weird_encoding(tangled_string),
            String::from("ABCXYZ")
        );
    }

    #[test]
    fn untangle_tangled_string_with_nul() {
        let tangled_string = String::from("0x41 0x42  0x43  0x00  0x44");
        assert_eq!(
            Battery::untangle_weird_encoding(tangled_string),
            String::from("ABC D")
        );
    }
}
