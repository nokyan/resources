use anyhow::{Context, Result};
use gtk::glib::DateTime;
use ini::Ini;
use log::debug;
use once_cell::sync::Lazy;
use process_data::unix_as_millis;

pub mod app;
pub mod battery;
pub mod cpu;
pub mod drive;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod pci;
pub mod process;
pub mod settings;
pub mod units;

const FLATPAK_SPAWN: &str = "/usr/bin/flatpak-spawn";

static BOOT_TIMESTAMP: Lazy<Option<i64>> = Lazy::new(|| {
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

static TICK_RATE: Lazy<usize> =
    Lazy::new(|| sysconf::sysconf(sysconf::SysconfVariable::ScClkTck).unwrap_or(100) as usize);

static FLATPAK_APP_PATH: Lazy<String> =
    Lazy::new(|| flatpak_app_path().unwrap_or_else(|_| String::new()));

pub static NUM_CPUS: Lazy<usize> = Lazy::new(num_cpus::get);

// Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
pub static IS_FLATPAK: Lazy<bool> = Lazy::new(|| {
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

pub trait NaNDefault {
    /// Returns the given `default` value if the variable is NaN,
    /// and returns itself otherwise.
    #[must_use]
    fn nan_default(&self, default: Self) -> Self;
}

impl NaNDefault for f64 {
    fn nan_default(&self, default: Self) -> Self {
        if self.is_nan() {
            default
        } else {
            *self
        }
    }
}

impl NaNDefault for f32 {
    fn nan_default(&self, default: Self) -> Self {
        if self.is_nan() {
            default
        } else {
            *self
        }
    }
}
