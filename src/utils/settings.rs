use std::{ops::Deref, str::FromStr, sync::LazyLock};

use adw::prelude::*;

use gtk::{gio, glib};
use strum_macros::{Display, EnumString, FromRepr};

use crate::config::APP_ID;

pub static SETTINGS: LazyLock<Settings> = LazyLock::new(Settings::default);

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, EnumString, Display, Hash, FromRepr)]
pub enum Base {
    #[default]
    Decimal,
    Binary,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, EnumString, Display, Hash, FromRepr)]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Kelvin,
    Fahrenheit,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, EnumString, Display, Hash, FromRepr)]
pub enum RefreshSpeed {
    VerySlow,
    Slow,
    #[default]
    Normal,
    Fast,
    VeryFast,
}

impl RefreshSpeed {
    pub fn ui_refresh_interval(&self) -> f32 {
        match self {
            RefreshSpeed::VerySlow => 3.0,
            RefreshSpeed::Slow => 2.0,
            RefreshSpeed::Normal => 1.0,
            RefreshSpeed::Fast => 0.5,
            RefreshSpeed::VeryFast => 0.25,
        }
    }

    pub fn process_refresh_interval(&self) -> f32 {
        self.ui_refresh_interval() * 2.0
    }
}

#[derive(Clone, Debug, Hash)]
pub struct Settings(gio::Settings);

impl Settings {
    pub fn temperature_unit(&self) -> TemperatureUnit {
        TemperatureUnit::from_str(self.string("temperature-unit").as_str()).unwrap_or_default()
    }

    pub fn set_temperature_unit(
        &self,
        temperature_unit: TemperatureUnit,
    ) -> Result<(), glib::error::BoolError> {
        self.set_string("temperature-unit", &temperature_unit.to_string())
    }

    pub fn connect_temperature_unit<F: Fn(TemperatureUnit) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_changed(Some("temperature-unit"), move |settings, _key| {
            f(
                TemperatureUnit::from_str(settings.string("temperature-unit").as_str())
                    .unwrap_or_default(),
            )
        })
    }

    pub fn base(&self) -> Base {
        Base::from_str(self.string("base").as_str()).unwrap_or_default()
    }

    pub fn set_base(&self, base: Base) -> Result<(), glib::error::BoolError> {
        self.set_string("base", &base.to_string())
    }

    pub fn connect_base<F: Fn(Base) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_changed(Some("base"), move |settings, _key| {
            f(Base::from_str(settings.string("base").as_str()).unwrap_or_default())
        })
    }

    pub fn refresh_speed(&self) -> RefreshSpeed {
        RefreshSpeed::from_str(self.string("refresh-speed").as_str()).unwrap_or_default()
    }

    pub fn set_refresh_speed(
        &self,
        refresh_speed: RefreshSpeed,
    ) -> Result<(), glib::error::BoolError> {
        self.set_string("refresh-speed", &refresh_speed.to_string())
    }

    pub fn connect_refresh_speed<F: Fn(RefreshSpeed) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_changed(Some("refresh-speed"), move |settings, _key| {
            f(
                RefreshSpeed::from_str(settings.string("refresh-speed").as_str())
                    .unwrap_or_default(),
            )
        })
    }

    pub fn window_width(&self) -> i32 {
        self.int("window-width")
    }

    pub fn set_window_width(&self, width: i32) -> Result<(), glib::error::BoolError> {
        self.set_int("window-width", width)
    }

    pub fn connect_window_width<F: Fn(i32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_changed(Some("window-width"), move |settings, _key| {
            f(settings.int("window-width"))
        })
    }

    pub fn window_height(&self) -> i32 {
        self.int("window-height")
    }

    pub fn set_window_height(&self, width: i32) -> Result<(), glib::error::BoolError> {
        self.set_int("window-height", width)
    }

    pub fn connect_window_height<F: Fn(i32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_changed(Some("window-height"), move |settings, _key| {
            f(settings.int("window-width"))
        })
    }

    pub fn is_maximized(&self) -> bool {
        self.boolean("is-maximized")
    }

    pub fn set_maximized(&self, maximized: bool) -> Result<(), glib::error::BoolError> {
        self.set_boolean("is-maximized", maximized)
    }

    pub fn connect_maximized<F: Fn(bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_changed(Some("is-maximized"), move |settings, _key| {
            f(settings.boolean("is-maximized"))
        })
    }
}

impl Deref for Settings {
    type Target = gio::Settings;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self(gio::Settings::new(APP_ID))
    }
}

unsafe impl Send for Settings {}
unsafe impl Sync for Settings {}
