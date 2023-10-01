use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};

use crate::i18n::i18n_f;

use super::settings::{Base, TemperatureUnit, SETTINGS};

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, EnumString, Display, Hash, EnumIter)]
enum Prefix {
    #[default]
    None,
    Kilo,
    Mega,
    Giga,
    Tera,
    Peta,
    Exa,
    Zetta,
    Yotta,
    Ronna,
    Quetta,
}

fn to_largest_prefix(amount: f64, prefix_base: Base) -> (f64, Prefix) {
    let mut x = amount;
    let base = match prefix_base {
        Base::Decimal => 1000.0,
        Base::Binary => 1024.0,
    };
    for prefix in Prefix::iter() {
        if x < base {
            return (x, prefix);
        }
        x /= base;
    }
    (x, Prefix::Quetta)
}

fn celsius_to_fahrenheit(celsius: f64) -> f64 {
    celsius * 1.8 + 32.0
}

fn celsius_to_kelvin(celsius: f64) -> f64 {
    celsius + 273.15
}

pub fn convert_temperature(celsius: f64) -> String {
    match SETTINGS.temperature_unit() {
        TemperatureUnit::Kelvin => {
            i18n_f("{} K", &[&(celsius_to_kelvin(celsius).round()).to_string()])
        }
        TemperatureUnit::Celsius => i18n_f("{} °C", &[&(celsius.round()).to_string()]),
        TemperatureUnit::Fahrenheit => i18n_f(
            "{} °F",
            &[&(celsius_to_fahrenheit(celsius).round()).to_string()],
        ),
    }
}

pub fn convert_storage(bytes: f64, integer: bool) -> String {
    match SETTINGS.base() {
        Base::Decimal => convert_storage_decimal(bytes, integer),
        Base::Binary => convert_storage_binary(bytes, integer),
    }
}

fn convert_storage_decimal(bytes: f64, integer: bool) -> String {
    let (mut number, prefix) = to_largest_prefix(bytes, Base::Decimal);
    if integer {
        number = number.round();
        match prefix {
            Prefix::None => i18n_f("{} B", &[&format!("{}", number)]),
            Prefix::Kilo => i18n_f("{} kB", &[&format!("{}", number)]),
            Prefix::Mega => i18n_f("{} MB", &[&format!("{}", number)]),
            Prefix::Giga => i18n_f("{} GB", &[&format!("{}", number)]),
            Prefix::Tera => i18n_f("{} TB", &[&format!("{}", number)]),
            Prefix::Peta => i18n_f("{} PB", &[&format!("{}", number)]),
            Prefix::Exa => i18n_f("{} EB", &[&format!("{}", number)]),
            Prefix::Zetta => i18n_f("{} ZB", &[&format!("{}", number)]),
            Prefix::Yotta => i18n_f("{} YB", &[&format!("{}", number)]),
            Prefix::Ronna => i18n_f("{} RB", &[&format!("{}", number)]),
            Prefix::Quetta => i18n_f("{} QB", &[&format!("{}", number)]),
        }
    } else {
        match prefix {
            Prefix::None => i18n_f("{} B", &[&format!("{}", number.round())]),
            Prefix::Kilo => i18n_f("{} kB", &[&format!("{:.2}", number)]),
            Prefix::Mega => i18n_f("{} MB", &[&format!("{:.2}", number)]),
            Prefix::Giga => i18n_f("{} GB", &[&format!("{:.2}", number)]),
            Prefix::Tera => i18n_f("{} TB", &[&format!("{:.2}", number)]),
            Prefix::Peta => i18n_f("{} PB", &[&format!("{:.2}", number)]),
            Prefix::Exa => i18n_f("{} EB", &[&format!("{:.2}", number)]),
            Prefix::Zetta => i18n_f("{} ZB", &[&format!("{:.2}", number)]),
            Prefix::Yotta => i18n_f("{} YB", &[&format!("{:.2}", number)]),
            Prefix::Ronna => i18n_f("{} RB", &[&format!("{:.2}", number)]),
            Prefix::Quetta => i18n_f("{} QB", &[&format!("{:.2}", number)]),
        }
    }
}

fn convert_storage_binary(bytes: f64, integer: bool) -> String {
    let (mut number, prefix) = to_largest_prefix(bytes, Base::Binary);
    if integer {
        number = number.round();
        match prefix {
            Prefix::None => i18n_f("{} B", &[&format!("{}", number)]),
            Prefix::Kilo => i18n_f("{} KiB", &[&format!("{}", number)]),
            Prefix::Mega => i18n_f("{} MiB", &[&format!("{}", number)]),
            Prefix::Giga => i18n_f("{} GiB", &[&format!("{}", number)]),
            Prefix::Tera => i18n_f("{} TiB", &[&format!("{}", number)]),
            Prefix::Peta => i18n_f("{} PiB", &[&format!("{}", number)]),
            Prefix::Exa => i18n_f("{} EiB", &[&format!("{}", number)]),
            Prefix::Zetta => i18n_f("{} ZiB", &[&format!("{}", number)]),
            Prefix::Yotta => i18n_f("{} YiB", &[&format!("{}", number)]),
            Prefix::Ronna => i18n_f("{} RiB", &[&format!("{}", number)]),
            Prefix::Quetta => i18n_f("{} QiB", &[&format!("{}", number)]),
        }
    } else {
        match prefix {
            Prefix::None => i18n_f("{} B", &[&format!("{}", number.round())]),
            Prefix::Kilo => i18n_f("{} KiB", &[&format!("{:.2}", number)]),
            Prefix::Mega => i18n_f("{} MiB", &[&format!("{:.2}", number)]),
            Prefix::Giga => i18n_f("{} GiB", &[&format!("{:.2}", number)]),
            Prefix::Tera => i18n_f("{} TiB", &[&format!("{:.2}", number)]),
            Prefix::Peta => i18n_f("{} PiB", &[&format!("{:.2}", number)]),
            Prefix::Exa => i18n_f("{} EiB", &[&format!("{:.2}", number)]),
            Prefix::Zetta => i18n_f("{} ZiB", &[&format!("{:.2}", number)]),
            Prefix::Yotta => i18n_f("{} YiB", &[&format!("{:.2}", number)]),
            Prefix::Ronna => i18n_f("{} RiB", &[&format!("{:.2}", number)]),
            Prefix::Quetta => i18n_f("{} QiB", &[&format!("{:.2}", number)]),
        }
    }
}

pub fn convert_speed(bytes_per_second: f64) -> String {
    match SETTINGS.base() {
        Base::Decimal => convert_speed_decimal(bytes_per_second),
        Base::Binary => convert_speed_binary(bytes_per_second),
    }
}

fn convert_speed_decimal(bytes_per_second: f64) -> String {
    let (number, prefix) = to_largest_prefix(bytes_per_second, Base::Decimal);
    match prefix {
        Prefix::None => i18n_f("{} B/s", &[&format!("{}", number.round())]),
        Prefix::Kilo => i18n_f("{} kB/s", &[&format!("{:.2}", number)]),
        Prefix::Mega => i18n_f("{} MB/s", &[&format!("{:.2}", number)]),
        Prefix::Giga => i18n_f("{} GB/s", &[&format!("{:.2}", number)]),
        Prefix::Tera => i18n_f("{} TB/s", &[&format!("{:.2}", number)]),
        Prefix::Peta => i18n_f("{} PB/s", &[&format!("{:.2}", number)]),
        Prefix::Exa => i18n_f("{} EB/s", &[&format!("{:.2}", number)]),
        Prefix::Zetta => i18n_f("{} ZB/s", &[&format!("{:.2}", number)]),
        Prefix::Yotta => i18n_f("{} YB/s", &[&format!("{:.2}", number)]),
        Prefix::Ronna => i18n_f("{} RB/s", &[&format!("{:.2}", number)]),
        Prefix::Quetta => i18n_f("{} QB/s", &[&format!("{:.2}", number)]),
    }
}

fn convert_speed_binary(bytes_per_second: f64) -> String {
    let (number, prefix) = to_largest_prefix(bytes_per_second, Base::Binary);
    match prefix {
        Prefix::None => i18n_f("{} B/s", &[&format!("{}", number.round())]),
        Prefix::Kilo => i18n_f("{} KiB/s", &[&format!("{:.2}", number)]),
        Prefix::Mega => i18n_f("{} MiB/s", &[&format!("{:.2}", number)]),
        Prefix::Giga => i18n_f("{} GiB/s", &[&format!("{:.2}", number)]),
        Prefix::Tera => i18n_f("{} TiB/s", &[&format!("{:.2}", number)]),
        Prefix::Peta => i18n_f("{} PiB/s", &[&format!("{:.2}", number)]),
        Prefix::Exa => i18n_f("{} EiB/s", &[&format!("{:.2}", number)]),
        Prefix::Zetta => i18n_f("{} ZiB/s", &[&format!("{:.2}", number)]),
        Prefix::Yotta => i18n_f("{} YiB/s", &[&format!("{:.2}", number)]),
        Prefix::Ronna => i18n_f("{} RiB/s", &[&format!("{:.2}", number)]),
        Prefix::Quetta => i18n_f("{} QiB/s", &[&format!("{:.2}", number)]),
    }
}

pub fn convert_frequency(hertz: f64) -> String {
    let (number, prefix) = to_largest_prefix(hertz, Base::Decimal);
    match prefix {
        Prefix::None => i18n_f("{} Hz", &[&format!("{:.2}", number)]),
        Prefix::Kilo => i18n_f("{} kHz", &[&format!("{:.2}", number)]),
        Prefix::Mega => i18n_f("{} MHz", &[&format!("{:.2}", number)]),
        Prefix::Giga => i18n_f("{} GHz", &[&format!("{:.2}", number)]),
        Prefix::Tera => i18n_f("{} THz", &[&format!("{:.2}", number)]),
        Prefix::Peta => i18n_f("{} PHz", &[&format!("{:.2}", number)]),
        Prefix::Exa => i18n_f("{} EHz", &[&format!("{:.2}", number)]),
        Prefix::Zetta => i18n_f("{} ZHz", &[&format!("{:.2}", number)]),
        Prefix::Yotta => i18n_f("{} YHz", &[&format!("{:.2}", number)]),
        Prefix::Ronna => i18n_f("{} RHz", &[&format!("{:.2}", number)]),
        Prefix::Quetta => i18n_f("{} QHz", &[&format!("{:.2}", number)]),
    }
}

pub fn convert_power(watts: f64) -> String {
    let (number, prefix) = to_largest_prefix(watts, Base::Decimal);
    match prefix {
        Prefix::None => i18n_f("{} W", &[&format!("{:.1}", number)]),
        Prefix::Kilo => i18n_f("{} kW", &[&format!("{:.2}", number)]),
        Prefix::Mega => i18n_f("{} MW", &[&format!("{:.2}", number)]),
        Prefix::Giga => i18n_f("{} GW", &[&format!("{:.2}", number)]),
        Prefix::Tera => i18n_f("{} TW", &[&format!("{:.2}", number)]),
        Prefix::Peta => i18n_f("{} PW", &[&format!("{:.2}", number)]),
        Prefix::Exa => i18n_f("{} EW", &[&format!("{:.2}", number)]),
        Prefix::Zetta => i18n_f("{} ZW", &[&format!("{:.2}", number)]),
        Prefix::Yotta => i18n_f("{} YW", &[&format!("{:.2}", number)]),
        Prefix::Ronna => i18n_f("{} RW", &[&format!("{:.2}", number)]),
        Prefix::Quetta => i18n_f("{} QW", &[&format!("{:.2}", number)]),
    }
}
