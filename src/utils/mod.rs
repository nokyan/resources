use std::{collections::HashMap, path::Path, str::FromStr, sync::LazyLock};

use anyhow::{Context, Result};
use gtk::glib::DateTime;
use ini::Ini;
use log::{debug, trace};
use process_data::unix_as_millis;

pub mod app;
pub mod battery;
pub mod cpu;
pub mod drive;
pub mod gpu;
pub mod link;
pub mod memory;
pub mod network;
pub mod npu;
pub mod os;
pub mod pci;
pub mod process;
pub mod settings;
pub mod units;

const FLATPAK_SPAWN: &str = "/usr/bin/flatpak-spawn";

static BOOT_TIMESTAMP: LazyLock<Option<i64>> = LazyLock::new(|| {
    let unix_timestamp = (unix_as_millis() / 1000) as i64;
    std::fs::read_to_string("/proc/uptime")
        .context("unable to read /proc/uptime")
        .and_then(|procfs| {
            procfs
                .split(' ')
                .next()
                .map(str::to_string)
                .context("unable to split /proc/uptime")
        })
        .and_then(|uptime_str| {
            uptime_str
                .parse::<f64>()
                .context("unable to parse /proc/uptime")
        })
        .map(|uptime_secs| unix_timestamp - uptime_secs as i64)
        .ok()
});

static FLATPAK_APP_PATH: LazyLock<String> =
    LazyLock::new(|| flatpak_app_path().unwrap_or_else(|_| String::new()));

pub static TICK_RATE: LazyLock<usize> =
    LazyLock::new(|| sysconf::sysconf(sysconf::SysconfVariable::ScClkTck).unwrap_or(100) as usize);

pub static NUM_CPUS: LazyLock<usize> = LazyLock::new(num_cpus::get);

// Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
pub static IS_FLATPAK: LazyLock<bool> = LazyLock::new(|| {
    trace!("Determining whether /.flatpak-info exists…");
    let is_flatpak = std::path::Path::new("/.flatpak-info").exists();

    if is_flatpak {
        debug!("Running as Flatpak");
    } else {
        debug!("Not running as Flatpak");
    }

    is_flatpak
});

// Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
pub fn flatpak_app_path() -> Result<String> {
    let ini = Ini::load_from_file("/.flatpak-info").context("unable to find ./flatpak-info")?;

    let section = ini
        .section(Some("Instance"))
        .context("unable to find Instance section in ./flatpak-info")?;

    Ok(section
        .get("app-path")
        .context("unable to find app-path in ./flatpak-info")?
        .to_string())
}

pub fn boot_time() -> Result<DateTime> {
    BOOT_TIMESTAMP
        .context("couldn't get boot timestamp")
        .and_then(|timestamp| {
            DateTime::from_unix_local(timestamp).context("unable to get glib::DateTime")
        })
}

pub fn read_uevent_contents(contents: impl AsRef<str>) -> Result<HashMap<String, String>> {
    contents
        .as_ref()
        .lines()
        .map(|line| {
            line.split_once('=')
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .context(format!("malformed line (no '='): {line}"))
        })
        .collect()
}

pub fn read_uevent(path: impl AsRef<Path>) -> Result<HashMap<String, String>> {
    let path = path.as_ref();

    trace!("Reading uevent contents of {}", path.display());

    read_uevent_contents(std::fs::read_to_string(path)?)
}

pub fn read_sysfs<T: FromStr>(path: impl AsRef<Path>) -> Result<T>
where
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    let path = path.as_ref();

    std::fs::read_to_string(path)?
        .trim_ascii_end()
        .parse::<T>()
        .with_context(|| format!("error parsing file {}", path.display()))
}

pub trait FiniteOr {
    /// Returns the given `x` value if the variable is NaN or infinite,
    /// and returns itself otherwise.
    #[must_use]
    fn finite_or(&self, x: Self) -> Self;

    /// Returns itself is the variable is finite (i.e. neither NaN nor infinite), otherwise returns its default
    fn finite_or_default(&self) -> Self;

    /// Returns itself is the variable is finite (i.e. neither NaN nor infinite), otherwise runs `f`
    fn finite_or_else<F: FnOnce(Self) -> Self>(&self, f: F) -> Self
    where
        Self: Sized;
}

impl FiniteOr for f64 {
    fn finite_or(&self, x: Self) -> Self {
        if self.is_finite() { *self } else { x }
    }

    fn finite_or_default(&self) -> Self {
        if self.is_finite() {
            *self
        } else {
            Self::default()
        }
    }

    fn finite_or_else<F: FnOnce(Self) -> Self>(&self, f: F) -> Self
    where
        Self: Sized,
    {
        if self.is_finite() { *self } else { f(*self) }
    }
}

impl FiniteOr for f32 {
    fn finite_or(&self, x: Self) -> Self {
        if self.is_finite() { *self } else { x }
    }

    fn finite_or_default(&self) -> Self {
        if self.is_finite() {
            *self
        } else {
            Self::default()
        }
    }

    fn finite_or_else<F: FnOnce(Self) -> Self>(&self, f: F) -> Self
    where
        Self: Sized,
    {
        if self.is_finite() { *self } else { f(*self) }
    }
}

#[cfg(test)]
mod test {
    use core::f64;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

    use crate::utils::{FiniteOr, read_uevent_contents};

    #[test]
    fn read_uevent_contents_valid_simple() {
        let uevent_raw = "a=b";

        let parsed = read_uevent_contents(uevent_raw).unwrap();

        let expected: HashMap<String, String> = HashMap::from([("a".into(), "b".into())]);

        assert_eq!(expected, parsed)
    }

    #[test]
    fn read_uevent_contents_valid_single_equals() {
        let uevent_raw = "=";

        let parsed = read_uevent_contents(uevent_raw).unwrap();

        let expected: HashMap<String, String> = HashMap::from([("".into(), "".into())]);

        assert_eq!(expected, parsed)
    }

    #[test]
    fn read_uevent_contents_valid_multiple_equals() {
        let uevent_raw = "a=b=c";

        let parsed = read_uevent_contents(uevent_raw).unwrap();

        let expected: HashMap<String, String> = HashMap::from([("a".into(), "b=c".into())]);

        assert_eq!(expected, parsed)
    }

    #[test]
    fn read_uevent_contents_valid_left_empty() {
        let uevent_raw = "=EMPTY";

        let parsed = read_uevent_contents(uevent_raw).unwrap();

        let expected: HashMap<String, String> = HashMap::from([("".into(), "EMPTY".into())]);

        assert_eq!(expected, parsed)
    }

    #[test]
    fn read_uevent_contents_valid_right_empty() {
        let uevent_raw = "EMPTY=";

        let parsed = read_uevent_contents(uevent_raw).unwrap();

        let expected: HashMap<String, String> = HashMap::from([("EMPTY".into(), "".into())]);

        assert_eq!(expected, parsed)
    }

    #[test]
    fn read_uevent_contents_valid_complex() {
        let uevent_raw = concat!(
            "DRIVER=driver\n",
            "PCI_CLASS=20000\n",
            "CONTAINS_EQUALS=a=b\n",
            "EMPTY=\n",
            "="
        );

        let parsed = read_uevent_contents(uevent_raw).unwrap();

        let expected: HashMap<String, String> = HashMap::from([
            ("DRIVER".into(), "driver".into()),
            ("PCI_CLASS".into(), "20000".into()),
            ("CONTAINS_EQUALS".into(), "a=b".into()),
            ("EMPTY".into(), "".into()),
            ("".into(), "".into()),
        ]);

        assert_eq!(expected, parsed)
    }

    #[test]
    fn read_uevent_contents_valid_empty() {
        let uevent_raw = "";

        let parsed = read_uevent_contents(uevent_raw).unwrap();

        let expected: HashMap<String, String> = HashMap::new();

        assert_eq!(expected, parsed)
    }

    #[test]
    fn read_uevent_contents_invalid() {
        let uevent_raw = "NO_EQUALS";

        let parsed = read_uevent_contents(uevent_raw);

        assert!(parsed.is_err())
    }

    #[test]
    fn finite_or_finite_f32() {
        let float: f32 = 1.0;

        let maybe = float.finite_or(8.0);

        assert_eq!(maybe, float);
    }

    #[test]
    fn finite_or_finite_f64() {
        let float: f64 = 1.0;

        let maybe = float.finite_or(8.0);

        assert_eq!(maybe, float);
    }

    #[test]
    fn finite_or_infinite_f32() {
        let float: f32 = f32::INFINITY;

        let maybe = float.finite_or(8.0);

        assert_eq!(maybe, 8.0);
    }

    #[test]
    fn finite_or_infinite_f64() {
        let float: f64 = f64::INFINITY;

        let maybe = float.finite_or(8.0);

        assert_eq!(maybe, 8.0);
    }

    #[test]
    fn finite_or_else_finite_f32() {
        let float: f32 = 1.0;

        let maybe = float.finite_or_else(|_| f32::powi(2.0, 3));

        assert_eq!(maybe, float);
    }

    #[test]
    fn finite_or_else_finite_f64() {
        let float: f64 = 1.0;

        let maybe = float.finite_or_else(|_| f64::powi(2.0, 3));

        assert_eq!(maybe, float);
    }

    #[test]
    fn finite_or_else_infinite_f32() {
        let float: f32 = f32::INFINITY;

        let maybe = float.finite_or_else(|_| f32::powi(2.0, 3));

        assert_eq!(maybe, 8.0);
    }

    #[test]
    fn finite_or_else_infinite_f64() {
        let float: f64 = f64::INFINITY;

        let maybe = float.finite_or_else(|_| f64::powi(2.0, 3));

        assert_eq!(maybe, 8.0);
    }

    #[test]
    fn finite_or_default_finite_f32() {
        let float: f32 = 1.0;

        let maybe = float.finite_or_default();

        assert_eq!(maybe, float);
    }

    #[test]
    fn finite_or_default_finite_f64() {
        let float: f64 = 1.0;

        let maybe = float.finite_or_default();

        assert_eq!(maybe, float);
    }

    #[test]
    fn finite_or_default_infinite_f32() {
        let float: f32 = f32::INFINITY;

        let maybe = float.finite_or_default();

        assert_eq!(maybe, f32::default());
    }

    #[test]
    fn finite_or_default_infinite_f64() {
        let float: f64 = f64::INFINITY;

        let maybe = float.finite_or_default();

        assert_eq!(maybe, f64::default());
    }
}
