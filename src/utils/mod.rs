use anyhow::{Context, Result};
use ini::Ini;
use once_cell::sync::Lazy;

pub mod app;
pub mod cpu;
pub mod drive;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod process;
pub mod settings;
pub mod units;

// Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
static IS_FLATPAK: Lazy<bool> = Lazy::new(|| std::path::Path::new("/.flatpak-info").exists());

static FLATPAK_APP_PATH: Lazy<String> =
    Lazy::new(|| flatpak_app_path().unwrap_or_else(|_| String::new()));

static FLATPAK_SPAWN: &str = "/usr/bin/flatpak-spawn";

// Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
pub fn flatpak_app_path() -> Result<String> {
    let ini =
        Ini::load_from_file("/.flatpak-info").with_context(|| "unable to find ./flatpak-info")?;

    let section = ini
        .section(Some("Instance"))
        .with_context(|| "unable to find Instance section in ./flatpak-info")?;

    Ok(section
        .get("app-path")
        .with_context(|| "unable to find app-path in ./flatpak-info")?
        .to_string())
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
