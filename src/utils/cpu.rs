use anyhow::{anyhow, Context, Result};
use lazy_static::lazy_static;
use nparse::*;
use regex::bytes::Regex;
use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct CPUInfo {
    pub vendor_id: Option<String>,
    pub model_name: Option<String>,
    pub architecture: Option<String>,
    pub logical_cpus: Option<usize>,
    pub physical_cpus: Option<usize>,
    pub sockets: Option<usize>,
    pub virtualization: Option<String>,
    pub max_speed: Option<f32>,
}

impl Default for CPUInfo {
    fn default() -> Self {
        Self {
            vendor_id: Default::default(),
            model_name: Default::default(),
            architecture: Default::default(),
            logical_cpus: Default::default(),
            physical_cpus: Default::default(),
            sockets: Default::default(),
            virtualization: Default::default(),
            max_speed: Default::default(),
        }
    }
}

fn lscpu() -> Result<Value> {
    String::from_utf8(Command::new("lscpu").output().unwrap().stdout)
        .with_context(|| "unable to parse lscpu output to UTF-8")?
        .kv_str_to_json()
        .map_err(|x| anyhow!("{}", x))
}

/// Returns a `CPUInfo` struct populated with values gathered from `lscpu`.
pub fn cpu_info() -> Result<CPUInfo> {
    let lscpu_output = lscpu()?;
    let vendor_id = lscpu_output["Vendor ID"]
        .as_str()
        .and_then(|x| Some(x.to_string()));
    let model_name = lscpu_output["Model name"]
        .as_str()
        .and_then(|x| Some(x.to_string()));
    let architecture = lscpu_output["Architecture"]
        .as_str()
        .and_then(|x| Some(x.to_string()));
    let logical_cpus = lscpu_output["CPU(s)"]
        .as_str()
        .and_then(|x| x.parse::<usize>().ok());
    let sockets = lscpu_output["Socket(s)"]
        .as_str()
        .and_then(|x| x.parse::<usize>().ok());
    let physical_cpus = lscpu_output["Core(s) per socket"].as_str().and_then(|x| {
        x.parse::<usize>()
            .ok()
            .and_then(|y| Some(y * sockets.unwrap_or(1)))
    });
    let virtualization = lscpu_output["Virtualization"]
        .as_str()
        .and_then(|x| Some(x.to_string()));
    let max_speed = lscpu_output["CPU max MHz"]
        .as_str()
        .and_then(|x| x.parse::<f32>().ok())
        .and_then(|y| Some(y * 1000000.0));
    Ok(CPUInfo {
        vendor_id,
        model_name,
        architecture,
        logical_cpus,
        physical_cpus,
        sockets,
        virtualization,
        max_speed,
    })
}

pub fn get_cpu_freq(core: usize) -> Result<u64> {
    std::fs::read_to_string(format!(
        "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq",
        core
    ))
    .with_context(|| format!("unable to read scaling_cur_freq for core {}", core))?
    .replace("\n", "")
    .parse::<u64>()
    .with_context(|| "can't parse scaling_cur_freq to usize")
    .and_then(|x| Ok(x * 1000))
}

fn parse_proc_stat_line(line: &[u8]) -> Result<(u64, u64)> {
    lazy_static! {
        static ref PROC_STAT_REGEX: Regex = Regex::new(r"cpu[0-9]* *(?P<user>[0-9]*) *(?P<nice>[0-9]*) *(?P<system>[0-9]*) *(?P<idle>[0-9]*) *(?P<iowait>[0-9]*) *(?P<irq>[0-9]*) *(?P<softirq>[0-9]*) *(?P<steal>[0-9]*) *(?P<guest>[0-9]*) *(?P<guest_nice>[0-9]*)").unwrap();
    }
    let captures = PROC_STAT_REGEX
        .captures(line)
        .ok_or(anyhow!("using regex to parse /proc/stat failed"))?;
    let idle_time = captures
        .name("idle")
        .and_then(|x| String::from_utf8_lossy(x.as_bytes()).parse::<u64>().ok())
        .ok_or(anyhow!("unable to get idle time"))?
        - captures
            .name("iowait")
            .and_then(|x| String::from_utf8_lossy(x.as_bytes()).parse::<u64>().ok())
            .ok_or(anyhow!("unable to get iowait time"))?;
    let sum = captures
        .iter()
        .skip(1)
        .map(|cap| {
            cap.and_then(|x| String::from_utf8_lossy(x.as_bytes()).parse::<u64>().ok())
                .ok_or(anyhow!("unable to sum CPU times from /proc/stat"))
                .unwrap() // TODO: get rid of this unwrap() somehow
        })
        .sum();
    Ok((idle_time, sum))
}

pub fn get_proc_stat(core: Option<usize>) -> Result<String> {
    // the combined stats are in line 0, the other cores are in the following lines,
    // since our `core` argument starts with 0, we must add 1 to it if it's not `None`.
    let selected_line_number = core.and_then(|x| Some(x + 1)).unwrap_or(0);
    let proc_stat_raw =
        std::fs::read_to_string("/proc/stat").with_context(|| "unable to read /proc/stat")?;
    let mut proc_stat = proc_stat_raw.split("\n").collect::<Vec<&str>>();
    proc_stat.retain(|x| x.starts_with("cpu"));
    // return an `Error` if `core` is greater than the number of cores
    if selected_line_number > proc_stat.len() {
        return Err(anyhow!("`core` argument greater than amount of cores"));
    }
    Ok(proc_stat[selected_line_number].to_string())
}

/// Returns the CPU usage of either all cores combined (if supplied argument is `None`),
/// or of a specific thread (taken from the supplied argument starting at 0)
/// Please keep in mind that this is the total CPU time since boot, you have to do delta
/// calculations yourself. The tuple's layout is: (idle_time, total_time)
pub fn get_cpu_usage(core: Option<usize>) -> Result<(u64, u64)> {
    parse_proc_stat_line(get_proc_stat(core)?.as_bytes())
}
