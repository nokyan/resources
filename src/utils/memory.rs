use std::process::Command;

use anyhow::{bail, Context, Result};
use nparse::KVStrToJson;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

use super::{FLATPAK_APP_PATH, FLATPAK_SPAWN, IS_FLATPAK};

static RE_SPEED: Lazy<Regex> = Lazy::new(|| Regex::new(r"Speed: (\d+) MT/s").unwrap());
static RE_FORMFACTOR: Lazy<Regex> = Lazy::new(|| Regex::new(r"Form Factor: (.+)").unwrap());
static RE_TYPE: Lazy<Regex> = Lazy::new(|| Regex::new(r"Type: (.+)").unwrap());
static RE_TYPE_DETAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"Type Detail: (.+)").unwrap());

#[derive(Debug)]
pub struct MemoryData {
    pub total_mem: usize,
    pub available_mem: usize,
    pub free_mem: usize,
    pub total_swap: usize,
    pub free_swap: usize,
}

impl MemoryData {
    pub fn new() -> Result<Self> {
        let values = std::fs::read_to_string("/proc/meminfo")?
            .kv_str_to_json()
            .map_err(anyhow::Error::msg)?;

        let total_mem = values["MemTotal"]
            .as_str()
            .and_then(|string| string.split(' ').nth(0))
            .and_then(|split| split.parse::<usize>().ok())
            .map(|int| int * 1000)
            .context("unable to read total memory")?;

        let available_mem = values["MemAvailable"]
            .as_str()
            .and_then(|string| string.split(' ').nth(0))
            .and_then(|split| split.parse::<usize>().ok())
            .map(|int| int * 1000)
            .context("unable to read total memory")?;

        let free_mem = values["MemFree"]
            .as_str()
            .and_then(|string| string.split(' ').nth(0))
            .and_then(|split| split.parse::<usize>().ok())
            .map(|int| int * 1000)
            .context("unable to read free memory")?;

        let total_swap = values["SwapTotal"]
            .as_str()
            .and_then(|string| string.split(' ').nth(0))
            .and_then(|split| split.parse::<usize>().ok())
            .map(|int| int * 1000)
            .context("unable to read total swap")?;

        let free_swap = values["SwapFree"]
            .as_str()
            .and_then(|string| string.split(' ').nth(0))
            .and_then(|split| split.parse::<usize>().ok())
            .map(|int| int * 1000)
            .context("unable to read free swap")?;

        Ok(Self {
            total_mem,
            available_mem,
            free_mem,
            total_swap,
            free_swap,
        })
    }
}

fn proc_meminfo() -> Result<Value, anyhow::Error> {
    std::fs::read_to_string("/proc/meminfo")
        .with_context(|| "unable to read /proc/meminfo")?
        .kv_str_to_json()
        .map_err(anyhow::Error::msg)
}

pub fn get_total_memory() -> Option<usize> {
    proc_meminfo().ok()?["MemTotal"]
        .as_str()
        .and_then(|x| x.split(' ').collect::<Vec<&str>>()[0].parse::<usize>().ok())
        .map(|y| y * 1000)
}

pub fn get_available_memory() -> Option<usize> {
    proc_meminfo().ok()?["MemAvailable"]
        .as_str()
        .and_then(|x| x.split(' ').collect::<Vec<&str>>()[0].parse::<usize>().ok())
        .map(|y| y * 1000)
}

pub fn get_free_memory() -> Option<usize> {
    proc_meminfo().ok()?["MemFree"]
        .as_str()
        .and_then(|x| x.split(' ').collect::<Vec<&str>>()[0].parse::<usize>().ok())
        .map(|y| y * 1000)
}

pub fn get_total_swap() -> Option<usize> {
    proc_meminfo().ok()?["SwapTotal"]
        .as_str()
        .and_then(|x| x.split(' ').collect::<Vec<&str>>()[0].parse::<usize>().ok())
        .map(|y| y * 1000)
}

pub fn get_free_swap() -> Option<usize> {
    proc_meminfo().ok()?["SwapFree"]
        .as_str()
        .and_then(|x| x.split(' ').collect::<Vec<&str>>()[0].parse::<usize>().ok())
        .map(|y| y * 1000)
}

#[derive(Debug, Clone, Default)]
pub struct MemoryDevice {
    pub speed: Option<u32>,
    pub form_factor: String,
    pub r#type: String,
    pub type_detail: String,
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
            speed: RE_SPEED
                .captures(device_string)
                .map(|x| x[1].parse().unwrap()),
            form_factor: RE_FORMFACTOR
                .captures(device_string)
                .map_or_else(|| "N/A".to_string(), |x| x[1].to_string()),
            r#type: RE_TYPE
                .captures(device_string)
                .map_or_else(|| "N/A".to_string(), |x| x[1].to_string()),
            type_detail: RE_TYPE_DETAIL
                .captures(device_string)
                .map_or_else(|| "N/A".to_string(), |x| x[1].to_string()),
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
        bail!("no permission")
    }
    Ok(parse_dmidecode(String::from_utf8(output.stdout)?.as_str()))
}

pub fn pkexec_get_memory_devices() -> Result<Vec<MemoryDevice>> {
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
