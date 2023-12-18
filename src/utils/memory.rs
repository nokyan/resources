use std::process::Command;

use anyhow::{bail, Context, Result};
use log::debug;
use once_cell::sync::Lazy;
use regex::Regex;

use super::{FLATPAK_APP_PATH, FLATPAK_SPAWN, IS_FLATPAK};

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
    pub speed: Option<u32>,
    pub form_factor: Option<String>,
    pub r#type: Option<String>,
    pub type_detail: Option<String>,
    pub installed: bool,
}

fn parse_dmidecode(dmi: &str) -> Vec<MemoryDevice> {
    let mut devices = Vec::new();

    let device_strings = dmi.split("\n\n");

    for device_string in device_strings {
        if device_string.is_empty() {
            continue;
        }
        let memory_device = MemoryDevice {
            speed: RE_CONFIGURED_SPEED
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

pub fn get_memory_devices() -> Result<Vec<MemoryDevice>> {
    let output = Command::new("dmidecode")
        .args(["-t", "17", "-q"])
        .output()?;
    if output.status.code().unwrap_or(1) == 1 {
        debug!("Unable to get memory information (dmidecode) without privileges");
        bail!("no permission")
    }
    Ok(parse_dmidecode(String::from_utf8(output.stdout)?.as_str()))
}

pub fn pkexec_get_memory_devices() -> Result<Vec<MemoryDevice>> {
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
    Ok(parse_dmidecode(String::from_utf8(output.stdout)?.as_str()))
}
