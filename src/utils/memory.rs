use anyhow::{Result, Context};
use nparse::KVStrToJson;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct MemInfo {
    pub mem_type: Option<String>,
    pub total_slots: Option<usize>,
    pub slots_used: Option<usize>,
    pub speed: Option<String>
}

pub fn mem_info() -> MemInfo {
    let mem_type = None;
    let total_slots = None;
    let slots_used = None;
    let speed = None;
    MemInfo {
        mem_type,
        total_slots,
        slots_used,
        speed,
    }
}

fn proc_meminfo() -> Result<Value, String> {
    std::fs::read_to_string("/proc/meminfo").with_context(|| "unable to read /proc/meminfo").unwrap().kv_str_to_json()
}

pub fn get_total_memory() -> Option<usize> {
    proc_meminfo().ok()?["MemTotal"].as_str().and_then(|x| x.split(" ").collect::<Vec<&str>>()[0].parse::<usize>().ok()).and_then(|y| Some(y * 1000))
}

pub fn get_available_memory() -> Option<usize> {
    proc_meminfo().ok()?["MemAvailable"].as_str().and_then(|x| x.split(" ").collect::<Vec<&str>>()[0].parse::<usize>().ok()).and_then(|y| Some(y * 1000))
}

pub fn get_free_memory() -> Option<usize> {
    proc_meminfo().ok()?["MemFree"].as_str().and_then(|x| x.split(" ").collect::<Vec<&str>>()[0].parse::<usize>().ok()).and_then(|y| Some(y * 1000))
}

pub fn get_total_swap() -> Option<usize> {
    proc_meminfo().ok()?["SwapTotal"].as_str().and_then(|x| x.split(" ").collect::<Vec<&str>>()[0].parse::<usize>().ok()).and_then(|y| Some(y * 1000))
}

pub fn get_free_swap() -> Option<usize> {
    proc_meminfo().ok()?["SwapFree"].as_str().and_then(|x| x.split(" ").collect::<Vec<&str>>()[0].parse::<usize>().ok()).and_then(|y| Some(y * 1000))
}