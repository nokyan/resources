use anyhow::{anyhow, bail, Context, Result};
use glob::glob;
use lazy_regex::{lazy_regex, Lazy, Regex};
use log::{debug, trace, warn};
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

const PROC_STAT: &str = "/proc/stat";

const KNOWN_HWMONS: &[&str] = &["zenpower", "coretemp", "k10temp"];

const KNOWN_THERMAL_ZONES: &[&str] = &["cpu-thermal", "x86_pkg_temp", "acpitz"];

static RE_LSCPU_MODEL_NAME: Lazy<Regex> = lazy_regex!(r"Model name:\s*(.*)");

static RE_LSCPU_ARCHITECTURE: Lazy<Regex> = lazy_regex!(r"Architecture:\s*(.*)");

static RE_LSCPU_CPUS: Lazy<Regex> = lazy_regex!(r"CPU\(s\):\s*(.*)");

static RE_LSCPU_SOCKETS: Lazy<Regex> = lazy_regex!(r"Socket\(s\):\s*(.*)");

static RE_LSCPU_CORES: Lazy<Regex> = lazy_regex!(r"Core\(s\) per socket:\s*(.*)");

static RE_LSCPU_VIRTUALIZATION: Lazy<Regex> = lazy_regex!(r"Virtualization:\s*(.*)");

static RE_LSCPU_MAX_MHZ: Lazy<Regex> = lazy_regex!(r"CPU max MHz:\s*(.*)");

static RE_PROC_STAT: Lazy<Regex> = lazy_regex!(
    r"cpu[0-9]+ *(?P<user>[0-9]*) *(?P<nice>[0-9]*) *(?P<system>[0-9]*) *(?P<idle>[0-9]*) *(?P<iowait>[0-9]*) *(?P<irq>[0-9]*) *(?P<softirq>[0-9]*) *(?P<steal>[0-9]*) *(?P<guest>[0-9]*) *(?P<guest_nice>[0-9]*)"
);

static CPU_TEMPERATURE_PATH: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    let cpu_temperature_path =
        search_for_hwmons(KNOWN_HWMONS).or_else(|| search_for_thermal_zones(KNOWN_THERMAL_ZONES));

    if let Some((sensor, path)) = &cpu_temperature_path {
        debug!(
            "CPU temperature sensor located at {} ({sensor})",
            path.display()
        );
    } else {
        warn!("No sensor for CPU temperature found!");
    }

    cpu_temperature_path.map(|(_, path)| path)
});

/// Looks for hwmons with the given names.
/// This function is a bit inefficient since the `names` array is considered to be ordered by priority.
fn search_for_hwmons(names: &[&'static str]) -> Option<(&'static str, PathBuf)> {
    for temp_name in names {
        for path in (glob("/sys/class/hwmon/hwmon*").unwrap()).flatten() {
            if let Ok(read_name) = std::fs::read_to_string(path.join("name")) {
                if &read_name.trim_end() == temp_name {
                    return Some((temp_name, path.join("temp1_input")));
                }
            }
        }
    }

    None
}

/// Looks for thermal zones with the given types.
/// This function is a bit inefficient since the `types` array is considered to be ordered by priority.
fn search_for_thermal_zones(types: &[&'static str]) -> Option<(&'static str, PathBuf)> {
    for temp_type in types {
        for path in (glob("/sys/class/thermal/thermal_zone*").unwrap()).flatten() {
            if let Ok(read_type) = std::fs::read_to_string(path.join("type")) {
                if &read_type.trim_end() == temp_type {
                    return Some((temp_type, path.join("temp")));
                }
            }
        }
    }

    None
}

#[derive(Debug)]
pub struct CpuData {
    pub new_thread_usages: Vec<Result<(u64, u64)>>,
    pub temperature: Result<f32, anyhow::Error>,
    pub frequencies: Vec<Option<u64>>,
}

impl CpuData {
    pub fn new(logical_cpus: usize) -> Self {
        trace!("Gathering CPU data…");
        let new_thread_usages = get_cpu_usage();

        let temperature = get_temperature();

        let mut frequencies = Vec::with_capacity(logical_cpus);

        for i in 0..logical_cpus {
            let freq = get_cpu_freq(i);
            frequencies.push(freq.ok());
        }

        let cpu_data = Self {
            new_thread_usages,
            temperature,
            frequencies,
        };

        trace!("Gathered CPU data: {cpu_data:?}");

        cpu_data
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuInfo {
    pub model_name: Option<String>,
    pub architecture: Option<String>,
    pub logical_cpus: Option<usize>,
    pub physical_cpus: Option<usize>,
    pub sockets: Option<usize>,
    pub virtualization: Option<String>,
    pub max_speed: Option<f64>,
}

impl CpuInfo {
    fn parse_lscpu<S: AsRef<str>>(lscpu_output: S) -> Self {
        let lscpu_output = lscpu_output.as_ref();

        let model_name = RE_LSCPU_MODEL_NAME
            .captures(lscpu_output)
            .and_then(|captures| {
                captures
                    .get(1)
                    .map(|capture| Self::trade_mark_symbols(capture.as_str()))
            });

        let architecture = RE_LSCPU_ARCHITECTURE
            .captures(lscpu_output)
            .and_then(|captures| captures.get(1).map(|capture| capture.as_str().into()));

        let sockets = RE_LSCPU_SOCKETS
            .captures(lscpu_output)
            .and_then(|captures| {
                captures
                    .get(1)
                    .and_then(|capture| capture.as_str().parse().ok())
            });

        let logical_cpus = RE_LSCPU_CPUS.captures(lscpu_output).and_then(|captures| {
            captures
                .get(1)
                .and_then(|capture| capture.as_str().parse().ok())
        });

        let physical_cpus = RE_LSCPU_CORES.captures(lscpu_output).and_then(|captures| {
            captures
                .get(1)
                .and_then(|capture| capture.as_str().parse::<usize>().ok())
                .map(|int| int.saturating_mul(sockets.unwrap_or(1)))
        });

        let virtualization = RE_LSCPU_VIRTUALIZATION
            .captures(lscpu_output)
            .and_then(|captures| captures.get(1).map(|capture| capture.as_str().into()));

        let max_speed = RE_LSCPU_MAX_MHZ
            .captures(lscpu_output)
            .and_then(|captures| {
                captures.get(1).and_then(|capture| {
                    capture
                        .as_str()
                        .parse::<f64>()
                        .ok()
                        .map(|float| float * 1_000_000.0)
                })
            });

        Self {
            model_name,
            architecture,
            logical_cpus,
            physical_cpus,
            sockets,
            virtualization,
            max_speed,
        }
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
    pub fn get() -> Result<Self> {
        String::from_utf8(
            std::process::Command::new("lscpu")
                .env("LC_ALL", "C")
                .output()
                .context("unable to run lscpu, is util-linux installed?")?
                .stdout,
        )
        .context("unable to parse lscpu output to UTF-8")
        .map(Self::parse_lscpu)
    }
}

/// Returns the frequency of the given CPU `core`
///
/// # Errors
///
/// Will return `Err` if the are problems during reading or parsing
/// of the corresponding file in sysfs
pub fn get_cpu_freq(core: usize) -> Result<u64> {
    trace!("Finding CPU frequency for core {core}…");

    std::fs::read_to_string(format!(
        "/sys/devices/system/cpu/cpu{core}/cpufreq/scaling_cur_freq"
    ))
    .inspect_err(|err| trace!("Unable to get CPU frequency for core {core}: {err}"))
    .with_context(|| format!("unable to read scaling_cur_freq for core {core}"))?
    .replace('\n', "")
    .parse::<u64>()
    .context("can't parse scaling_cur_freq to usize")
    .map(|x| x * 1000)
    .inspect(|freq| trace!("Frequency of core {core}: {freq} Hz"))
}

fn parse_proc_stat_line<S: AsRef<str>>(line: S) -> Result<(u64, u64)> {
    let captures = RE_PROC_STAT
        .captures(line.as_ref())
        .context("using regex to parse /proc/stat failed")?;

    let idle = captures
        .name("idle")
        .and_then(|x| x.as_str().parse::<u64>().ok())
        .context("unable to get idle time")?;

    let iowait = captures
        .name("iowait")
        .and_then(|x| x.as_str().parse::<u64>().ok())
        .context("unable to get idle time")?;

    let total = captures
        .iter()
        .skip(1)
        .flat_map(|cap| {
            cap.and_then(|x| x.as_str().parse::<u64>().ok())
                .ok_or_else(|| anyhow!("unable to sum CPU times from /proc/stat"))
        })
        .sum();

    Ok((idle.saturating_add(iowait), total))
}

fn parse_proc_stat<S: AsRef<str>>(stat: S) -> Vec<Result<(u64, u64)>> {
    trace!("Parsing {PROC_STAT}…");

    stat.as_ref()
        .lines()
        .skip(1)
        .filter(|line| line.starts_with("cpu"))
        .map(|line| parse_proc_stat_line(line))
        .collect()
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
pub fn get_cpu_usage() -> Vec<Result<(u64, u64)>> {
    trace!("Reading {PROC_STAT}…");

    let raw = std::fs::read_to_string("/proc/stat")
        .context("unable to read /proc/stat")
        .unwrap_or_default();

    parse_proc_stat(raw)
}

/// Returns the CPU temperature.
///
/// # Errors
///
/// Will return `Err` if there was no way to read the CPU temperature.
pub fn get_temperature() -> Result<f32> {
    if let Some(path) = CPU_TEMPERATURE_PATH.as_ref() {
        read_sysfs_thermal(path)
    } else {
        bail!("no CPU temperature sensor found")
    }
}

fn read_sysfs_thermal<P: AsRef<Path>>(path: P) -> Result<f32> {
    let path = path.as_ref();
    let temp_string = std::fs::read_to_string(path)
        .with_context(|| format!("unable to read {}", path.display()))?;
    temp_string
        .replace('\n', "")
        .parse::<f32>()
        .with_context(|| format!("unable to parse {}", path.display()))
        .map(|t| t / 1000f32)
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::utils::cpu::CpuInfo;

    const LSCPU_OUTPUT: &str = concat!(
        "Architecture:             x86_64\n",
        "  CPU op-mode(s):         32-bit, 64-bit\n",
        "  Address sizes:          48 bits physical, 48 bits virtual\n",
        "  Byte Order:             Little Endian\n",
        "CPU(s):                   16\n",
        "  On-line CPU(s) list:    0-7\n",
        "Vendor ID:                UnauthenticIngenuineManufacturer\n",
        "  Model name:             UIM(R) Abacus(tm) 10\n",
        "    CPU family:           1\n",
        "    Model:                2\n",
        "    Thread(s) per core:   2\n",
        "    Core(s) per socket:   4\n",
        "    Socket(s):            2\n",
        "    Stepping:             2\n",
        "    Frequency boost:      enabled\n",
        "    CPU(s) scaling MHz:   100%\n",
        "    CPU max MHz:          3.0000\n",
        "    CPU min MHz:          2.0000\n",
        "    BogoMIPS:             0.0\n",
        "Virtualization features:  \n",
        "  Virtualization:         Abacus-V\n",
        "Caches (sum of all):      \n",
        "  L1d:                    256 KiB (8 instances)\n",
        "  L1i:                    256 KiB (8 instances)\n",
        "  L2:                     4 MiB (8 instances)\n",
        "  L3:                     32 MiB (1 instance)\n",
        "NUMA:                     \n",
        "  NUMA node(s):           1\n",
        "  NUMA node0 CPU(s):      0-15\n",
    );

    #[test]
    fn lscpu_complex() {
        let parsed = CpuInfo::parse_lscpu(LSCPU_OUTPUT);

        let expected = CpuInfo {
            model_name: Some("UIM® Abacus™ 10".into()),
            architecture: Some("x86_64".into()),
            logical_cpus: Some(16),
            physical_cpus: Some(8),
            sockets: Some(2),
            virtualization: Some("Abacus-V".into()),
            max_speed: Some(3000000.0),
        };

        assert_eq!(parsed, expected)
    }
}
