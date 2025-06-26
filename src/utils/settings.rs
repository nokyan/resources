use std::{ops::Deref, str::FromStr, sync::LazyLock};

use adw::prelude::*;

use gtk::{SortType, gio, glib};
use log::debug;
use strum_macros::{Display, EnumString, FromRepr};

use paste::paste;

use crate::config::APP_ID;

pub static SETTINGS: LazyLock<Settings> = LazyLock::new(Settings::default);

macro_rules! bool_settings {
    ($($setting_name:ident),*) => {
        $(
            pub fn $setting_name(&self) -> bool {
                self.boolean(&stringify!($setting_name).replace("_", "-"))
            }

            paste! {
                pub fn [<set_ $setting_name>](&self, value: bool) -> Result<(), glib::error::BoolError> {
                    debug!("Setting boolean {} to {value}", stringify!($setting_name).replace("_", "-"));
                    self.set_boolean(&stringify!($setting_name).replace("_", "-"), value)
                }

                pub fn [<connect_ $setting_name>]<F: Fn(bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
                    self.connect_changed(
                        Some(&stringify!($setting_name).replace("_", "-")),
                        move |settings, _key| {
                            f(settings.boolean(&stringify!($setting_name).replace("_", "-")))
                        },
                    )
                }
            }
        )*
    };
}

macro_rules! int_settings {
    ($($setting_name:ident),*) => {
        $(
            pub fn $setting_name(&self) -> i32 {
                self.int(&stringify!($setting_name).replace("_", "-"))
            }

            paste! {
                pub fn [<set_ $setting_name>](&self, value: i32) -> Result<(), glib::error::BoolError> {
                    debug!("Setting int {} to {value}", stringify!($setting_name).replace("_", "-"));
                    self.set_int(&stringify!($setting_name).replace("_", "-"), value)
                }

                pub fn [<connect_ $setting_name>]<F: Fn(i32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
                    self.connect_changed(
                        Some(&stringify!($setting_name).replace("_", "-")),
                        move |settings, _key| {
                            f(settings.int(&stringify!($setting_name).replace("_", "-")))
                        },
                    )
                }
            }
        )*
    };
}

macro_rules! uint_settings {
    ($($setting_name:ident),*) => {
        $(
            pub fn $setting_name(&self) -> u32 {
                self.uint(&stringify!($setting_name).replace("_", "-"))
            }

            paste! {
                pub fn [<set_ $setting_name>](&self, value: u32) -> Result<(), glib::error::BoolError> {
                    debug!("Setting uint {} to {value}", stringify!($setting_name).replace("_", "-"));
                    self.set_uint(&stringify!($setting_name).replace("_", "-"), value)
                }

                pub fn [<connect_ $setting_name>]<F: Fn(u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
                    self.connect_changed(
                        Some(&stringify!($setting_name).replace("_", "-")),
                        move |settings, _key| {
                            f(settings.uint(&stringify!($setting_name).replace("_", "-")))
                        },
                    )
                }
            }
        )*
    };
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, EnumString, Display, Hash, FromRepr)]
pub enum Base {
    #[default]
    Decimal,
    Binary,
}

impl Base {
    pub const fn base(&self) -> f64 {
        match self {
            Base::Decimal => 1000.0,
            Base::Binary => 1024.0,
        }
    }
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
    pub const fn ui_refresh_interval(&self) -> f32 {
        match self {
            RefreshSpeed::VerySlow => 3.0,
            RefreshSpeed::Slow => 2.0,
            RefreshSpeed::Normal => 1.0,
            RefreshSpeed::Fast => 0.5,
            RefreshSpeed::VeryFast => 0.25,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, EnumString, Display, Hash, FromRepr)]
pub enum SidebarMeterType {
    #[default]
    ProgressBar,
    Graph,
}

#[derive(Clone, Debug, Hash)]
pub struct Settings(gio::Settings);

impl Settings {
    pub fn temperature_unit(&self) -> TemperatureUnit {
        TemperatureUnit::from_str(self.string("temperature-unit").as_str()).unwrap_or_default()
    }

    pub fn set_temperature_unit(
        &self,
        value: TemperatureUnit,
    ) -> Result<(), glib::error::BoolError> {
        debug!("Setting temperature-unit to {value}");
        self.set_string("temperature-unit", &value.to_string())
    }

    pub fn connect_temperature_unit<F: Fn(TemperatureUnit) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_changed(Some("temperature-unit"), move |settings, _key| {
            f(
                TemperatureUnit::from_str(settings.string("temperature-unit").as_str())
                    .unwrap_or_default(),
            );
        })
    }

    pub fn base(&self) -> Base {
        Base::from_str(self.string("base").as_str()).unwrap_or_default()
    }

    pub fn set_base(&self, value: Base) -> Result<(), glib::error::BoolError> {
        debug!("Setting base to {value}");
        self.set_string("base", &value.to_string())
    }

    pub fn connect_base<F: Fn(Base) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_changed(Some("base"), move |settings, _key| {
            f(Base::from_str(settings.string("base").as_str()).unwrap_or_default());
        })
    }

    pub fn last_viewed_page(&self) -> String {
        self.string("last-viewed-page").to_string()
    }

    pub fn set_last_viewed_page<S: AsRef<str>>(
        &self,
        value: S,
    ) -> Result<(), glib::error::BoolError> {
        debug!("Setting last-viewed-page to {}", value.as_ref());
        self.set_string("last-viewed-page", value.as_ref())
    }

    pub fn connect_last_viewed_page<F: Fn(String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_changed(Some("last-viewed-page"), move |settings, _key| {
            f(settings.string("last-viewed-page").to_string());
        })
    }

    pub fn refresh_speed(&self) -> RefreshSpeed {
        RefreshSpeed::from_str(self.string("refresh-speed").as_str()).unwrap_or_default()
    }

    pub fn set_refresh_speed(&self, value: RefreshSpeed) -> Result<(), glib::error::BoolError> {
        debug!("Setting refresh-speed to {value}");
        self.set_string("refresh-speed", &value.to_string())
    }

    pub fn connect_refresh_speed<F: Fn(RefreshSpeed) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_changed(Some("refresh-speed"), move |settings, _key| {
            f(
                RefreshSpeed::from_str(settings.string("refresh-speed").as_str())
                    .unwrap_or_default(),
            );
        })
    }

    pub fn sidebar_meter_type(&self) -> SidebarMeterType {
        SidebarMeterType::from_str(self.string("sidebar-meter-type").as_str()).unwrap_or_default()
    }

    pub fn set_sidebar_meter_type(
        &self,
        value: SidebarMeterType,
    ) -> Result<(), glib::error::BoolError> {
        debug!("Setting sidebar-meter-type to {value}");
        self.set_string("sidebar-meter-type", &value.to_string())
    }

    pub fn connect_sidebar_meter_type<F: Fn(SidebarMeterType) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_changed(Some("sidebar-meter-type"), move |settings, _key| {
            f(
                SidebarMeterType::from_str(settings.string("sidebar-meter-type").as_str())
                    .unwrap_or_default(),
            );
        })
    }

    // the following three functions are kept for compatibility reasons and for not having an oddly named function
    // called "set_is_maximized" generated by the macro
    pub fn maximized(&self) -> bool {
        self.boolean("is-maximized")
    }

    pub fn set_maximized(&self, value: bool) -> Result<(), glib::error::BoolError> {
        debug!("Setting boolean is-maximized to {value}");
        self.set_boolean("is-maximized", value)
    }

    pub fn connect_maximized<F: Fn(bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_changed(Some("is-maximized"), move |settings, _key| {
            f(settings.boolean("is-maximized"));
        })
    }

    pub fn processes_sort_by_ascending(&self) -> SortType {
        if self.boolean("processes-sort-by-ascending") {
            SortType::Ascending
        } else {
            SortType::Descending
        }
    }

    pub fn set_processes_sort_by_ascending(
        &self,
        value: SortType,
    ) -> Result<(), glib::error::BoolError> {
        let setting = matches!(value, SortType::Ascending);
        debug!("Setting boolean processes-sort-by-ascending to {setting}");
        self.set_boolean("processes-sort-by-ascending", setting)
    }

    pub fn connect_processes_sort_by_ascending<F: Fn(SortType) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_changed(
            Some("processes-sort-by-ascending"),
            move |settings, _key| {
                let sort_type = if settings.boolean("processes-sort-by-ascending") {
                    SortType::Ascending
                } else {
                    SortType::Descending
                };

                f(sort_type);
            },
        )
    }

    pub fn apps_sort_by_ascending(&self) -> SortType {
        if self.boolean("apps-sort-by-ascending") {
            SortType::Ascending
        } else {
            SortType::Descending
        }
    }

    pub fn set_apps_sort_by_ascending(
        &self,
        value: SortType,
    ) -> Result<(), glib::error::BoolError> {
        let setting = matches!(value, SortType::Ascending);
        debug!("Setting boolean apps-sort-by-ascending to {setting}");
        self.set_boolean("apps-sort-by-ascending", setting)
    }

    pub fn connect_apps_sort_by_ascending<F: Fn(SortType) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_changed(Some("apps-sort-by-ascending"), move |settings, _key| {
            let sort_type = if settings.boolean("apps-sort-by-ascending") {
                SortType::Ascending
            } else {
                SortType::Descending
            };

            f(sort_type);
        })
    }

    int_settings!(window_width, window_height);

    uint_settings!(graph_data_points, apps_sort_by, processes_sort_by);

    bool_settings!(
        show_virtual_drives,
        show_virtual_network_interfaces,
        sidebar_details,
        sidebar_description,
        network_bits,
        apps_show_memory,
        apps_show_cpu,
        apps_show_drive_read_speed,
        apps_show_drive_read_total,
        apps_show_drive_write_speed,
        apps_show_drive_write_total,
        apps_show_gpu,
        apps_show_gpu_memory,
        apps_show_encoder,
        apps_show_decoder,
        apps_show_swap,
        apps_show_combined_memory,
        processes_show_id,
        processes_show_user,
        processes_show_memory,
        processes_show_cpu,
        processes_show_drive_read_speed,
        processes_show_drive_read_total,
        processes_show_drive_write_speed,
        processes_show_drive_write_total,
        processes_show_gpu,
        processes_show_gpu_memory,
        processes_show_encoder,
        processes_show_decoder,
        processes_show_total_cpu_time,
        processes_show_user_cpu_time,
        processes_show_system_cpu_time,
        processes_show_priority,
        processes_show_swap,
        processes_show_combined_memory,
        show_logical_cpus,
        show_graph_grids,
        normalize_cpu_usage,
        detailed_priority
    );
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
