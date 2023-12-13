use anyhow::{anyhow, bail, Context, Result};
use glob::glob;
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::PathBuf;
use std::sync::OnceLock;

static RE_LSCPU_MODEL_NAME: Lazy<Regex> = Lazy::new(|| Regex::new(r"Model name:\s*(.*)").unwrap());

static RE_LSCPU_ARCHITECTURE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Architecture:\s*(.*)").unwrap());

static RE_LSCPU_CPUS: Lazy<Regex> = Lazy::new(|| Regex::new(r"CPU\(s\):\s*(.*)").unwrap());

static RE_LSCPU_SOCKETS: Lazy<Regex> = Lazy::new(|| Regex::new(r"Socket\(s\):\s*(.*)").unwrap());

static RE_LSCPU_CORES: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Core\(s\) per socket:\s*(.*)").unwrap());

static RE_LSCPU_VIRTUALIZATION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Virtualization:\s*(.*)").unwrap());

static RE_LSCPU_MAX_MHZ: Lazy<Regex> = Lazy::new(|| Regex::new(r"CPU max MHz:\s*(.*)").unwrap());

static RE_PROC_STAT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"cpu[0-9]* *(?P<user>[0-9]*) *(?P<nice>[0-9]*) *(?P<system>[0-9]*) *(?P<idle>[0-9]*) *(?P<iowait>[0-9]*) *(?P<irq>[0-9]*) *(?P<softirq>[0-9]*) *(?P<steal>[0-9]*) *(?P<guest>[0-9]*) *(?P<guest_nice>[0-9]*)").unwrap()
});

static ZENPOWER: OnceLock<PathBuf> = OnceLock::new();
static CORETEMP: OnceLock<PathBuf> = OnceLock::new();
static K10TEMP: OnceLock<PathBuf> = OnceLock::new();
static X86_PKG_TEMP: OnceLock<PathBuf> = OnceLock::new();
static ACPI: OnceLock<PathBuf> = OnceLock::new();

pub struct CpuData {
    pub new_total_usage: (u64, u64),
    pub new_thread_usages: Vec<(u64, u64)>,
    pub temperature: Result<f32, anyhow::Error>,
    pub frequencies: Vec<u64>,
}

impl CpuData {
    pub fn new(logical_cpus: usize) -> Self {
        let new_total_usage = get_cpu_usage(None).unwrap_or((0, 0));

        let temperature = get_temperature();

        let mut frequencies = vec![];
        let mut new_thread_usages = vec![];
        for i in 0..logical_cpus {
            let smth = get_cpu_usage(Some(i)).unwrap_or((0, 0));
            new_thread_usages.push(smth);

            let freq = get_cpu_freq(i).unwrap_or(0);
            frequencies.push(freq);
        }

        Self {
            new_total_usage,
            new_thread_usages,
            temperature,
            frequencies,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CpuInfo {
    pub model_name: Option<String>,
    pub architecture: Option<String>,
    pub logical_cpus: Option<usize>,
    pub physical_cpus: Option<usize>,
    pub sockets: Option<usize>,
    pub virtualization: Option<String>,
    pub max_speed: Option<f64>,
}

fn trade_mark_symbols<S: AsRef<str>>(s: S) -> String {
    s.as_ref()
        .replace("(R)", "®")
        .replace("(tm)", "™")
        .replace("(TM)", "™")
}

/// Returns a `CPUInfo` struct populated with values gathered from `lscpu`.
///
/// # Errors
///
/// Will return `Err` if the are problems during reading or parsing
/// of the `lscpu` command
pub fn cpu_info() -> Result<CpuInfo> {
    let lscpu_output = String::from_utf8(
        std::process::Command::new("lscpu")
            .env("LC_ALL", "C")
            .output()
            .context("unable to run lscpu, is util-linux installed?")?
            .stdout,
    )
    .context("unable to parse lscpu output to UTF-8")?;

    let model_name = RE_LSCPU_MODEL_NAME
        .captures(&lscpu_output)
        .and_then(|captures| {
            captures
                .get(1)
                .map(|capture| trade_mark_symbols(capture.as_str()))
        });

    let architecture = RE_LSCPU_ARCHITECTURE
        .captures(&lscpu_output)
        .and_then(|captures| captures.get(1).map(|capture| capture.as_str().into()));

    let sockets = RE_LSCPU_SOCKETS
        .captures(&lscpu_output)
        .and_then(|captures| {
            captures
                .get(1)
                .and_then(|capture| capture.as_str().parse().ok())
        });

    let logical_cpus = RE_LSCPU_CPUS.captures(&lscpu_output).and_then(|captures| {
        captures.get(1).and_then(|capture| {
            capture
                .as_str()
                .parse::<usize>()
                .ok()
                .map(|int| int * sockets.unwrap_or(1))
        })
    });

    let physical_cpus = RE_LSCPU_CORES.captures(&lscpu_output).and_then(|captures| {
        captures
            .get(1)
            .and_then(|capture| capture.as_str().parse().ok())
    });

    let virtualization = RE_LSCPU_VIRTUALIZATION
        .captures(&lscpu_output)
        .and_then(|captures| captures.get(1).map(|capture| capture.as_str().into()));

    let max_speed = RE_LSCPU_MAX_MHZ
        .captures(&lscpu_output)
        .and_then(|captures| {
            captures.get(1).and_then(|capture| {
                capture
                    .as_str()
                    .parse::<f64>()
                    .ok()
                    .map(|float| float * 1_000_000.0)
            })
        });

    Ok(CpuInfo {
        model_name,
        architecture,
        logical_cpus,
        physical_cpus,
        sockets,
        virtualization,
        max_speed,
    })
}

/// Returns the frequency of the given CPU `core`
///
/// # Errors
///
/// Will return `Err` if the are problems during reading or parsing
/// of the corresponding file in sysfs
pub fn get_cpu_freq(core: usize) -> Result<u64> {
    std::fs::read_to_string(format!(
        "/sys/devices/system/cpu/cpu{core}/cpufreq/scaling_cur_freq"
    ))
    .with_context(|| format!("unable to read scaling_cur_freq for core {core}"))?
    .replace('\n', "")
    .parse::<u64>()
    .context("can't parse scaling_cur_freq to usize")
    .map(|x| x * 1000)
}

fn parse_proc_stat_line<S: AsRef<str>>(line: S) -> Result<(u64, u64)> {
    let captures = RE_PROC_STAT
        .captures(line.as_ref())
        .ok_or_else(|| anyhow!("using regex to parse /proc/stat failed"))?;
    let idle_time = captures
        .name("idle")
        .and_then(|x| x.as_str().parse::<u64>().ok())
        .ok_or_else(|| anyhow!("unable to get idle time"))?
        + captures
            .name("iowait")
            .and_then(|x| x.as_str().parse::<u64>().ok())
            .ok_or_else(|| anyhow!("unable to get iowait time"))?;
    let sum = captures
        .iter()
        .skip(1)
        .flat_map(|cap| {
            cap.and_then(|x| x.as_str().parse::<u64>().ok())
                .ok_or_else(|| anyhow!("unable to sum CPU times from /proc/stat"))
        })
        .sum();
    Ok((idle_time, sum))
}

fn get_proc_stat(core: Option<usize>) -> Result<String> {
    // the combined stats are in line 0, the other cores are in the following lines,
    // since our `core` argument starts with 0, we must add 1 to it if it's not `None`.
    let selected_line_number = core.map_or(0, |x| x + 1);
    let proc_stat_raw =
        std::fs::read_to_string("/proc/stat").context("unable to read /proc/stat")?;
    let mut proc_stat = proc_stat_raw.split('\n').collect::<Vec<&str>>();
    proc_stat.retain(|x| x.starts_with("cpu"));
    // return an `Error` if `core` is greater than the number of cores
    if selected_line_number >= proc_stat.len() {
        bail!("`core` argument greater than amount of cores")
    }
    Ok(proc_stat[selected_line_number].to_string())
}

/// Returns the CPU usage of either all cores combined (if supplied argument is `None`),
/// or of a specific thread (taken from the supplied argument starting at 0)
/// Please keep in mind that this is the total CPU time since boot, you have to do delta
/// calculations yourself. The tuple's layout is: `(idle_time, total_time)`
///
/// # Errors
///
/// Will return `Err` if the are problems during reading or parsing
/// of /proc/stat
pub fn get_cpu_usage(core: Option<usize>) -> Result<(u64, u64)> {
    parse_proc_stat_line(get_proc_stat(core)?)
}

/// Returns the CPU temperature.
///
/// # Errors
///
/// Will return `Err` if there was no way to read the CPU temperature.
pub fn get_temperature() -> Result<f32> {
    if ZENPOWER.get().is_none()
        && CORETEMP.get().is_none()
        && ACPI.get().is_none()
        && X86_PKG_TEMP.get().is_none()
    {
        // collect all the known hwmons
        for path in (glob("/sys/class/hwmon/hwmon*")?).flatten() {
            match std::fs::read_to_string(path.join("name")).as_deref() {
                Ok("zenpower\n") => std::mem::drop(ZENPOWER.set(path.join("temp1_input"))),
                Ok("coretemp\n") => std::mem::drop(CORETEMP.set(path.join("temp1_input"))),
                Ok("k10temp\n") => std::mem::drop(K10TEMP.set(path.join("temp1_input"))),
                Ok(_) | Err(_) => {
                    continue;
                }
            };
        }

        // collect all the known thermal zones
        for path in (glob("/sys/class/thermal/thermal_zone*")?).flatten() {
            match std::fs::read_to_string(path.join("type")).as_deref() {
                Ok("x86_pkg_temp\n") => std::mem::drop(X86_PKG_TEMP.set(path.join("temp"))),
                Ok("acpitz\n") => std::mem::drop(ACPI.set(path.join("temp"))),
                Ok(_) | Err(_) => {
                    continue;
                }
            };
        }
    }

    if let Some(path) = ZENPOWER.get() {
        return read_sysfs_thermal(path);
    }
    if let Some(path) = K10TEMP.get() {
        return read_sysfs_thermal(path);
    }
    if let Some(path) = CORETEMP.get() {
        return read_sysfs_thermal(path);
    }
    if let Some(path) = X86_PKG_TEMP.get() {
        return read_sysfs_thermal(path);
    }
    if let Some(path) = ACPI.get() {
        return read_sysfs_thermal(path);
    }

    bail!("no CPU temperature sensor found")
}

fn read_sysfs_thermal(path: &PathBuf) -> Result<f32> {
    let temp_string = std::fs::read_to_string(path)
        .with_context(|| format!("unable to read {}", path.display()))?;
    temp_string
        .replace('\n', "")
        .parse::<f32>()
        .with_context(|| format!("unable to parse {}", path.display()))
        .map(|t| t / 1000f32)
}
