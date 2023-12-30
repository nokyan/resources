use std::process::Command;

use anyhow::{bail, Context, Result};
use log::debug;
use once_cell::sync::Lazy;
use regex::Regex;

use super::{FLATPAK_APP_PATH, FLATPAK_SPAWN, IS_FLATPAK};

const TEMPLATE_RE_PRESENT: &str = r"MEMORY_DEVICE_%_PRESENT=(\d)";

const TEMPLATE_RE_CONFIGURED_SPEED_MTS: &str = r"MEMORY_DEVICE_%_CONFIGURED_SPEED_MTS=(\d*)";

const TEMPLATE_RE_SPEED_MTS: &str = r"MEMORY_DEVICE_%_SPEED_MTS=(\d*)";

const TEMPLATE_RE_FORM_FACTOR: &str = r"MEMORY_DEVICE_%_FORM_FACTOR=(.*)";

const TEMPLATE_RE_TYPE: &str = r"MEMORY_DEVICE_%_TYPE=(.*)";

const TEMPLATE_RE_TYPE_DETAIL: &str = r"MEMORY_DEVICE_%_TYPE_DETAIL=(.*)";

static RE_CONFIGURED_SPEED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Configured Memory Speed: (\d+) MT/s").unwrap());

static RE_SPEED: Lazy<Regex> = Lazy::new(|| Regex::new(r"Speed: (\d+) MT/s").unwrap());

static RE_FORMFACTOR: Lazy<Regex> = Lazy::new(|| Regex::new(r"Form Factor: (.+)").unwrap());

static RE_TYPE: Lazy<Regex> = Lazy::new(|| Regex::new(r"Type: (.+)").unwrap());

static RE_TYPE_DETAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"Type Detail: (.+)").unwrap());

static RE_MEM_TOTAL: Lazy<Regex> = Lazy::new(|| Regex::new(r"MemTotal:\s*(\d*) kB").unwrap());

static RE_MEM_AVAILABLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"MemAvailable:\s*(\d*) kB").unwrap());

static RE_SWAP_TOTAL: Lazy<Regex> = Lazy::new(|| Regex::new(r"SwapTotal:\s*(\d*) kB").unwrap());

static RE_SWAP_FREE: Lazy<Regex> = Lazy::new(|| Regex::new(r"SwapFree:\s*(\d*) kB").unwrap());

static RE_NUM_MEMORY_DEVICES: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"MEMORY_ARRAY_NUM_DEVICES=(\d*)").unwrap());

#[derive(Debug)]
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
                            .map(|int| int * 1000)
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
                            .map(|int| int * 1000)
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
                            .map(|int| int * 1000)
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
                            .map(|int| int * 1000)
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

#[derive(Debug, Clone, Default)]
pub struct MemoryDevice {
    pub speed_mts: Option<u32>,
    pub form_factor: Option<String>,
    pub r#type: Option<String>,
    pub type_detail: Option<String>,
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

        let speed = Regex::new(&TEMPLATE_RE_CONFIGURED_SPEED_MTS.replace('%', &i))
            .ok()
            .and_then(|regex| regex.captures(dmi))
            .or_else(|| {
                Regex::new(&TEMPLATE_RE_SPEED_MTS.replace('%', &i.to_string()))
                    .ok()
                    .and_then(|regex| regex.captures(dmi))
            })
            .and_then(|captures| captures.get(1))
            .and_then(|capture| capture.as_str().parse().ok());

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

        let installed = Regex::new(&TEMPLATE_RE_PRESENT.replace('%', &i))
            .ok()
            .and_then(|regex| regex.captures(dmi))
            .and_then(|captures| captures.get(1))
            .and_then(|capture| capture.as_str().parse::<usize>().ok())
            .map(|int| int != 0)
            .unwrap_or(true);

        devices.push(MemoryDevice {
            speed_mts: speed,
            form_factor,
            r#type,
            type_detail,
            installed,
        })
    }

    devices
}

pub fn get_memory_devices() -> Result<Vec<MemoryDevice>> {
    let virtual_dmi = virtual_dmi();
    if !virtual_dmi.is_empty() {
        debug!("Memory information obtained using udevadm");
        Ok(virtual_dmi)
    } else {
        let output = Command::new("dmidecode")
            .args(["-t", "17", "-q"])
            .output()?;
        if output.status.code().unwrap_or(1) == 1 {
            debug!("Unable to get memory information without elevated privileges");
            bail!("no permission")
        }
        debug!("Memory information obtained using dmidecode (unprivileged)");
        Ok(parse_dmidecode(String::from_utf8(output.stdout)?))
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
