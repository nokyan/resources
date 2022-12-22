use anyhow::{Context, Result};
use nparse::KVStrToJson;
use serde_json::Value;

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
