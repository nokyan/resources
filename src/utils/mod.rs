use std::sync::OnceLock;

use anyhow::{Context, Result};
use ini::Ini;

pub mod cpu;
pub mod drive;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod processes;
pub mod settings;
pub mod units;

static IS_FLATPAK: OnceLock<bool> = OnceLock::new();

static FLATPAK_APP_PATH: OnceLock<String> = OnceLock::new();

static FLATPAK_SPAWN: &str = "/usr/bin/flatpak-spawn";

pub fn is_flatpak() -> bool {
    // Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
    *IS_FLATPAK.get_or_init(|| std::path::Path::new("/.flatpak-info").exists())
}

pub fn flatpak_app_path() -> String {
    // Adapted from Mission Center: https://gitlab.com/mission-center-devs/mission-center/
    FLATPAK_APP_PATH
        .get_or_try_init(|| -> Result<String> {
            let ini = Ini::load_from_file("/.flatpak-info")
                .with_context(|| "unable to find ./flatpak-info")?;

            let section = ini
                .section(Some("Instance"))
                .with_context(|| "unable to find Instance section in ./flatpak-info")?;

            Ok(section
                .get("app-path")
                .with_context(|| "unable to find app-path in ./flatpak-info")?
                .to_string())
        })
        .map_or_else(|_| String::new(), std::string::ToString::to_string)
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
