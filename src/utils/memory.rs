use std::process::Command;

use anyhow::{bail, Context, Result};
use lazy_regex::{lazy_regex, Lazy, Regex};
use log::debug;

use super::{FLATPAK_APP_PATH, FLATPAK_SPAWN, IS_FLATPAK};

const TEMPLATE_RE_PRESENT: &str = r"MEMORY_DEVICE_%_PRESENT=(\d)";

const TEMPLATE_RE_CONFIGURED_SPEED_MTS: &str = r"MEMORY_DEVICE_%_CONFIGURED_SPEED_MTS=(\d*)";

const TEMPLATE_RE_SPEED_MTS: &str = r"MEMORY_DEVICE_%_SPEED_MTS=(\d*)";

const TEMPLATE_RE_FORM_FACTOR: &str = r"MEMORY_DEVICE_%_FORM_FACTOR=(.*)";

const TEMPLATE_RE_TYPE: &str = r"MEMORY_DEVICE_%_TYPE=(.*)";

const TEMPLATE_RE_TYPE_DETAIL: &str = r"MEMORY_DEVICE_%_TYPE_DETAIL=(.*)";

const TEMPLATE_RE_SIZE: &str = r"MEMORY_DEVICE_%_SIZE=(\d*)";

const BYTES_IN_GIB: u64 = 1_073_741_824; // 1024 * 1024 * 1024

static RE_CONFIGURED_SPEED: Lazy<Regex> = lazy_regex!(r"Configured Memory Speed: (\d+) MT/s");

static RE_SPEED: Lazy<Regex> = lazy_regex!(r"Speed: (\d+) MT/s");

static RE_FORMFACTOR: Lazy<Regex> = lazy_regex!(r"Form Factor: (.+)");

static RE_TYPE: Lazy<Regex> = lazy_regex!(r"Type: (.+)");

static RE_TYPE_DETAIL: Lazy<Regex> = lazy_regex!(r"Type Detail: (.+)");

static RE_SIZE: Lazy<Regex> = lazy_regex!(r"Size: (\d+) GB");

static RE_MEM_TOTAL: Lazy<Regex> = lazy_regex!(r"MemTotal:\s*(\d*) kB");

static RE_MEM_AVAILABLE: Lazy<Regex> = lazy_regex!(r"MemAvailable:\s*(\d*) kB");

static RE_SWAP_TOTAL: Lazy<Regex> = lazy_regex!(r"SwapTotal:\s*(\d*) kB");

static RE_SWAP_FREE: Lazy<Regex> = lazy_regex!(r"SwapFree:\s*(\d*) kB");

static RE_NUM_MEMORY_DEVICES: Lazy<Regex> = lazy_regex!(r"MEMORY_ARRAY_NUM_DEVICES=(\d*)");

#[derive(Debug, Clone, Copy)]
pub struct MemoryData {
    pub total_mem: usize,
    pub available_mem: usize,
    pub total_swap: usize,
    pub free_swap: usize,
}

impl MemoryData {
    pub fn new() -> Result<Self> {
        let proc_mem =
            std::fs::read_to_string("/proc/meminfo").context("unable to read /proc/meminfo")?;

        let total_mem = RE_MEM_TOTAL
            .captures(&proc_mem)
            .context("RE_MEM_TOTAL no captures")
            .and_then(|captures| {
                captures
                    .get(1)
                    .context("RE_MEM_TOTAL not enough captures")
                    .and_then(|capture| {
                        capture
                            .as_str()
                            .parse::<usize>()
                            .context("unable to parse MemTotal")
                            .map(|int| int.saturating_mul(1024))
                    })
            })?;

        let available_mem = RE_MEM_AVAILABLE
            .captures(&proc_mem)
            .context("RE_MEM_AVAILABLE no captures")
            .and_then(|captures| {
                captures
                    .get(1)
                    .context("RE_MEM_AVAILABLE not enough captures")
                    .and_then(|capture| {
                        capture
                            .as_str()
                            .parse::<usize>()
                            .context("unable to parse MemAvailable")
                            .map(|int| int.saturating_mul(1024))
                    })
            })?;

        let total_swap = RE_SWAP_TOTAL
            .captures(&proc_mem)
            .context("RE_SWAP_TOTAL no captures")
            .and_then(|captures| {
                captures
                    .get(1)
                    .context("RE_SWAP_TOTAL not enough captures")
                    .and_then(|capture| {
                        capture
                            .as_str()
                            .parse::<usize>()
                            .context("unable to parse SwapTotal")
                            .map(|int| int.saturating_mul(1024))
                    })
            })?;

        let free_swap = RE_SWAP_FREE
            .captures(&proc_mem)
            .context("RE_SWAP_FREE no captures")
            .and_then(|captures| {
                captures
                    .get(1)
                    .context("RE_SWAP_FREE not enough captures")
                    .and_then(|capture| {
                        capture
                            .as_str()
                            .parse::<usize>()
                            .context("unable to parse SwapFree")
                            .map(|int| int.saturating_mul(1024))
                    })
            })?;

        Ok(Self {
            total_mem,
            available_mem,
            total_swap,
            free_swap,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryDevice {
    pub speed_mts: Option<u32>,
    pub form_factor: Option<String>,
    pub r#type: Option<String>,
    pub type_detail: Option<String>,
    pub size: Option<u64>,
    pub installed: bool,
}

fn parse_dmidecode<S: AsRef<str>>(dmi: S) -> Vec<MemoryDevice> {
    let mut devices = Vec::new();

    let device_strings = dmi.as_ref().split("\n\n");

    for device_string in device_strings {
        if device_string.is_empty() {
            continue;
        }
        let memory_device = MemoryDevice {
            speed_mts: RE_CONFIGURED_SPEED
                .captures(device_string)
                .or_else(|| RE_SPEED.captures(device_string))
                .map(|x| x[1].parse().unwrap()),
            form_factor: RE_FORMFACTOR
                .captures(device_string)
                .map(|x| x[1].to_string()),
            r#type: RE_TYPE.captures(device_string).map(|x| x[1].to_string()),
            type_detail: RE_TYPE_DETAIL
                .captures(device_string)
                .map(|x| x[1].to_string()),
            size: RE_SIZE
                .captures(device_string)
                .map(|x| x[1].parse::<u64>().unwrap() * BYTES_IN_GIB),
            installed: RE_SPEED
                .captures(device_string)
                .map(|x| x[1].to_string())
                .is_some(),
        };

        devices.push(memory_device);
    }

    devices
}

fn virtual_dmi() -> Vec<MemoryDevice> {
    let command = if *IS_FLATPAK {
        Command::new(FLATPAK_SPAWN)
            .args([
                "--host",
                "udevadm",
                "info",
                "-p",
                "/sys/devices/virtual/dmi/id",
            ])
            .output()
    } else {
        Command::new("udevadm")
            .args(["info", "-p", "/sys/devices/virtual/dmi/id"])
            .output()
    };

    let virtual_dmi_output = command
        .context("unable to execute udevadm")
        .and_then(|output| {
            String::from_utf8(output.stdout).context("unable to parse stdout of udevadm to UTF-8")
        })
        .unwrap_or_default();

    parse_virtual_dmi(virtual_dmi_output)
}

fn parse_virtual_dmi<S: AsRef<str>>(dmi: S) -> Vec<MemoryDevice> {
    let dmi = dmi.as_ref();

    let devices_amount: usize = RE_NUM_MEMORY_DEVICES
        .captures(dmi)
        .and_then(|captures| captures.get(1))
        .and_then(|capture| capture.as_str().parse().ok())
        .unwrap_or(0);

    let mut devices = Vec::with_capacity(devices_amount);

    for i in 0..devices_amount {
        let i = i.to_string();

        let installed = Regex::new(&TEMPLATE_RE_PRESENT.replace('%', &i))
            .ok()
            .and_then(|regex| regex.captures(dmi))
            .and_then(|captures| captures.get(1))
            .and_then(|capture| capture.as_str().parse::<usize>().ok())
            .map_or(true, |int| int != 0);

        let speed = if installed {
            Regex::new(&TEMPLATE_RE_CONFIGURED_SPEED_MTS.replace('%', &i))
                .ok()
                .and_then(|regex| regex.captures(dmi))
                .or_else(|| {
                    Regex::new(&TEMPLATE_RE_SPEED_MTS.replace('%', &i.to_string()))
                        .ok()
                        .and_then(|regex| regex.captures(dmi))
                })
                .and_then(|captures| captures.get(1))
                .and_then(|capture| capture.as_str().parse().ok())
        } else {
            None
        };

        let form_factor = Regex::new(&TEMPLATE_RE_FORM_FACTOR.replace('%', &i))
            .ok()
            .and_then(|regex| regex.captures(dmi))
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str().to_string());

        let r#type = Regex::new(&TEMPLATE_RE_TYPE.replace('%', &i))
            .ok()
            .and_then(|regex| regex.captures(dmi))
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str().to_string())
            .filter(|capture| capture != "<OUT OF SPEC>");

        let type_detail = Regex::new(&TEMPLATE_RE_TYPE_DETAIL.replace('%', &i))
            .ok()
            .and_then(|regex| regex.captures(dmi))
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str().to_string());

        let size = Regex::new(&TEMPLATE_RE_SIZE.replace('%', &i))
            .ok()
            .and_then(|regex| regex.captures(dmi))
            .and_then(|captures| captures.get(1))
            .and_then(|capture| capture.as_str().parse().ok());

        devices.push(MemoryDevice {
            speed_mts: speed,
            form_factor,
            r#type,
            type_detail,
            size,
            installed,
        });
    }

    devices
}

pub fn get_memory_devices() -> Result<Vec<MemoryDevice>> {
    let virtual_dmi = virtual_dmi();
    if virtual_dmi.is_empty() {
        let output = Command::new("dmidecode")
            .args(["-t", "17", "-q"])
            .output()?;
        if output.status.code().unwrap_or(1) == 1 {
            debug!("Unable to get memory information without elevated privileges");
            bail!("no permission")
        }
        debug!("Memory information obtained using dmidecode (unprivileged)");
        Ok(parse_dmidecode(String::from_utf8(output.stdout)?))
    } else {
        debug!("Memory information obtained using udevadm");
        Ok(virtual_dmi)
    }
}

pub fn pkexec_dmidecode() -> Result<Vec<MemoryDevice>> {
    debug!("Using pkexec to get memory information (dmidecode)…");
    let output = if *IS_FLATPAK {
        Command::new(FLATPAK_SPAWN)
            .args([
                "--host",
                "/usr/bin/pkexec",
                "--disable-internal-agent",
                &format!("{}/bin/dmidecode", FLATPAK_APP_PATH.as_str()),
                "-t",
                "17",
                "-q",
            ])
            .output()?
    } else {
        Command::new("pkexec")
            .args(["--disable-internal-agent", "dmidecode", "-t", "17", "-q"])
            .output()?
    };
    debug!("Memory information obtained using dmidecode (privileged)");
    Ok(parse_dmidecode(String::from_utf8(output.stdout)?.as_str()))
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::utils::memory::{parse_virtual_dmi, MemoryDevice};

    use super::parse_dmidecode;

    const DMIDECODE_OUTPUT: &str = concat!(
        "Memory Device\n",
        "        Total Width: Unknown\n",
        "        Data Width: Unknown\n",
        "        Size: No Module Installed\n",
        "        Form Factor: Unknown\n",
        "        Set: None\n",
        "        Locator: DIMM 0\n",
        "        Bank Locator: P0 CHANNEL A\n",
        "        Type: Unknown\n",
        "        Type Detail: Unknown\n",
        "\n",
        "Memory Device\n",
        "        Total Width: 64 bits\n",
        "        Data Width: 64 bits\n",
        "        Size: 16 GB\n",
        "        Form Factor: DIMM\n",
        "        Set: None\n",
        "        Locator: DIMM 1\n",
        "        Bank Locator: P0 CHANNEL A\n",
        "        Type: DDR4\n",
        "        Type Detail: Synchronous Unbuffered (Unregistered)\n",
        "        Speed: 3000 MT/s\n",
        "        Manufacturer: Unknown\n",
        "        Serial Number: 00000000\n",
        "        Asset Tag: Not Specified\n",
        "        Part Number: 123\n",
        "        Rank: 1\n",
        "        Configured Memory Speed: 3000 MT/s\n",
        "        Minimum Voltage: 1.2 V\n",
        "        Maximum Voltage: 1.2 V\n",
        "        Configured Voltage: 1.2 V"
    );

    const UDEVADM_OUTPUT: &str = concat!(
        "E: MEMORY_ARRAY_LOCATION=System Board Or Motherboard\n",
        "E: MEMORY_ARRAY_MAX_CAPACITY=137438953472\n",
        "E: MEMORY_DEVICE_0_PRESENT=0\n",
        "E: MEMORY_DEVICE_0_FORM_FACTOR=Unknown\n",
        "E: MEMORY_DEVICE_0_LOCATOR=DIMM 0\n",
        "E: MEMORY_DEVICE_0_BANK_LOCATOR=P0 CHANNEL A\n",
        "E: MEMORY_DEVICE_0_TYPE=Unknown\n",
        "E: MEMORY_DEVICE_0_TYPE_DETAIL=Unknown\n",
        "E: MEMORY_DEVICE_0_SPEED_MTS=3000\n",
        "E: MEMORY_DEVICE_0_MANUFACTURER=Unknown\n",
        "E: MEMORY_DEVICE_0_SERIAL_NUMBER=Unknown\n",
        "E: MEMORY_DEVICE_0_ASSET_TAG=Not Specified\n",
        "E: MEMORY_DEVICE_0_PART_NUMBER=Unknown\n",
        "E: MEMORY_DEVICE_0_CONFIGURED_SPEED_MTS=3000\n",
        "E: MEMORY_DEVICE_1_TOTAL_WIDTH=64\n",
        "E: MEMORY_DEVICE_1_DATA_WIDTH=64\n",
        "E: MEMORY_DEVICE_1_SIZE=17179869184\n",
        "E: MEMORY_DEVICE_1_FORM_FACTOR=DIMM\n",
        "E: MEMORY_DEVICE_1_LOCATOR=DIMM 1\n",
        "E: MEMORY_DEVICE_1_BANK_LOCATOR=P0 CHANNEL A\n",
        "E: MEMORY_DEVICE_1_TYPE=DDR4\n",
        "E: MEMORY_DEVICE_1_TYPE_DETAIL=Synchronous Unbuffered (Unregistered)\n",
        "E: MEMORY_DEVICE_1_SPEED_MTS=3000\n",
        "E: MEMORY_DEVICE_1_MANUFACTURER=Unknown\n",
        "E: MEMORY_DEVICE_1_SERIAL_NUMBER=00000000\n",
        "E: MEMORY_DEVICE_1_ASSET_TAG=Not Specified\n",
        "E: MEMORY_DEVICE_1_PART_NUMBER=123\n",
        "E: MEMORY_DEVICE_1_RANK=1\n",
        "E: MEMORY_DEVICE_1_CONFIGURED_SPEED_MTS=3000\n",
        "E: MEMORY_DEVICE_1_MINIMUM_VOLTAGE=1\n",
        "E: MEMORY_DEVICE_1_MAXIMUM_VOLTAGE=1\n",
        "E: MEMORY_DEVICE_1_CONFIGURED_VOLTAGE=1\n",
        "E: MEMORY_DEVICE_2_PRESENT=0\n",
        "E: MEMORY_ARRAY_NUM_DEVICES=2"
    );

    #[test]
    fn valid_dmidecode_complex() {
        let parsed = parse_dmidecode(DMIDECODE_OUTPUT);

        let expected = vec![
            MemoryDevice {
                speed_mts: None,
                form_factor: Some("Unknown".into()),
                r#type: Some("Unknown".into()),
                type_detail: Some("Unknown".into()),
                size: None,
                installed: false,
            },
            MemoryDevice {
                speed_mts: Some(3000),
                form_factor: Some("DIMM".into()),
                r#type: Some("DDR4".into()),
                type_detail: Some("Synchronous Unbuffered (Unregistered)".into()),
                size: Some(17179869184),
                installed: true,
            },
        ];

        assert_eq!(expected, parsed);
    }

    #[test]
    fn valid_udevadm_complex() {
        let parsed = parse_virtual_dmi(UDEVADM_OUTPUT);

        let expected = vec![
            MemoryDevice {
                speed_mts: None,
                form_factor: Some("Unknown".into()),
                r#type: Some("Unknown".into()),
                type_detail: Some("Unknown".into()),
                size: None,
                installed: false,
            },
            MemoryDevice {
                speed_mts: Some(3000),
                form_factor: Some("DIMM".into()),
                r#type: Some("DDR4".into()),
                type_detail: Some("Synchronous Unbuffered (Unregistered)".into()),
                size: Some(17179869184),
                installed: true,
            },
        ];

        assert_eq!(expected, parsed);
    }

    #[test]
    fn udevadm_dmidecode_equal() {
        let dmidecode = parse_dmidecode(DMIDECODE_OUTPUT);
        let udevadm = parse_virtual_dmi(UDEVADM_OUTPUT);

        assert_eq!(dmidecode, udevadm);
    }
}
