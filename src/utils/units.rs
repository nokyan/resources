use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};

use crate::i18n::i18n_f;

use super::settings::{Base, SETTINGS, TemperatureUnit};

#[repr(u8)]
#[derive(
    Debug, Clone, Copy, Default, EnumString, Display, Hash, EnumIter, PartialEq, PartialOrd, Eq, Ord,
)]
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

pub fn format_time(time_in_seconds: f64) -> String {
    if time_in_seconds.is_nan() || time_in_seconds.is_infinite() {
        return time_in_seconds.to_string().replace("inf", "∞");
    }
    let negative = time_in_seconds.is_sign_negative();
    let time_in_seconds = time_in_seconds.abs();

    let millis = ((time_in_seconds - time_in_seconds.floor()) * 100.0) as u8;
    let seconds = (time_in_seconds % 60.0) as u8;
    let minutes = ((time_in_seconds / 60.0) % 60.0) as u8;
    let hours = (time_in_seconds / (60.0 * 60.0)) as usize;

    if negative {
        format!("-{hours}∶{minutes:02}∶{seconds:02}.{millis:02}")
    } else {
        format!("{hours}∶{minutes:02}∶{seconds:02}.{millis:02}")
    }
}

fn to_largest_prefix(amount: f64, prefix_base: Base) -> (f64, Prefix) {
    if amount.is_nan() || amount.is_infinite() {
        return (amount, Prefix::None);
    }

    let negative_factor = if amount.is_sign_negative() { -1.0 } else { 1.0 };
    let mut x = amount.abs();
    let base = prefix_base.base();

    for prefix in Prefix::iter() {
        if x < base {
            return (x * negative_factor, prefix);
        }
        x /= base;
    }

    (x * negative_factor, Prefix::Quetta)
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

pub fn convert_storage_decimal(bytes: f64, integer: bool) -> String {
    let (mut number, prefix) = to_largest_prefix(bytes, Base::Decimal);
    if integer {
        number = number.round();
        match prefix {
            Prefix::None => i18n_f("{} B", &[&number.to_string()]),
            Prefix::Kilo => i18n_f("{} kB", &[&number.to_string()]),
            Prefix::Mega => i18n_f("{} MB", &[&number.to_string()]),
            Prefix::Giga => i18n_f("{} GB", &[&number.to_string()]),
            Prefix::Tera => i18n_f("{} TB", &[&number.to_string()]),
            Prefix::Peta => i18n_f("{} PB", &[&number.to_string()]),
            Prefix::Exa => i18n_f("{} EB", &[&number.to_string()]),
            Prefix::Zetta => i18n_f("{} ZB", &[&number.to_string()]),
            Prefix::Yotta => i18n_f("{} YB", &[&number.to_string()]),
            Prefix::Ronna => i18n_f("{} RB", &[&number.to_string()]),
            Prefix::Quetta => i18n_f("{} QB", &[&number.to_string()]),
        }
    } else {
        match prefix {
            Prefix::None => i18n_f("{} B", &[&format!("{}", number.round())]),
            Prefix::Kilo => i18n_f("{} kB", &[&format!("{number:.2}")]),
            Prefix::Mega => i18n_f("{} MB", &[&format!("{number:.2}")]),
            Prefix::Giga => i18n_f("{} GB", &[&format!("{number:.2}")]),
            Prefix::Tera => i18n_f("{} TB", &[&format!("{number:.2}")]),
            Prefix::Peta => i18n_f("{} PB", &[&format!("{number:.2}")]),
            Prefix::Exa => i18n_f("{} EB", &[&format!("{number:.2}")]),
            Prefix::Zetta => i18n_f("{} ZB", &[&format!("{number:.2}")]),
            Prefix::Yotta => i18n_f("{} YB", &[&format!("{number:.2}")]),
            Prefix::Ronna => i18n_f("{} RB", &[&format!("{number:.2}")]),
            Prefix::Quetta => i18n_f("{} QB", &[&format!("{number:.2}")]),
        }
    }
}

pub fn convert_storage_binary(bytes: f64, integer: bool) -> String {
    let (mut number, prefix) = to_largest_prefix(bytes, Base::Binary);
    if integer {
        number = number.round();
        match prefix {
            Prefix::None => i18n_f("{} B", &[&number.to_string()]),
            Prefix::Kilo => i18n_f("{} KiB", &[&number.to_string()]),
            Prefix::Mega => i18n_f("{} MiB", &[&number.to_string()]),
            Prefix::Giga => i18n_f("{} GiB", &[&number.to_string()]),
            Prefix::Tera => i18n_f("{} TiB", &[&number.to_string()]),
            Prefix::Peta => i18n_f("{} PiB", &[&number.to_string()]),
            Prefix::Exa => i18n_f("{} EiB", &[&number.to_string()]),
            Prefix::Zetta => i18n_f("{} ZiB", &[&number.to_string()]),
            Prefix::Yotta => i18n_f("{} YiB", &[&number.to_string()]),
            Prefix::Ronna => i18n_f("{} RiB", &[&number.to_string()]),
            Prefix::Quetta => i18n_f("{} QiB", &[&number.to_string()]),
        }
    } else {
        match prefix {
            Prefix::None => i18n_f("{} B", &[&format!("{}", number.round())]),
            Prefix::Kilo => i18n_f("{} KiB", &[&format!("{number:.2}")]),
            Prefix::Mega => i18n_f("{} MiB", &[&format!("{number:.2}")]),
            Prefix::Giga => i18n_f("{} GiB", &[&format!("{number:.2}")]),
            Prefix::Tera => i18n_f("{} TiB", &[&format!("{number:.2}")]),
            Prefix::Peta => i18n_f("{} PiB", &[&format!("{number:.2}")]),
            Prefix::Exa => i18n_f("{} EiB", &[&format!("{number:.2}")]),
            Prefix::Zetta => i18n_f("{} ZiB", &[&format!("{number:.2}")]),
            Prefix::Yotta => i18n_f("{} YiB", &[&format!("{number:.2}")]),
            Prefix::Ronna => i18n_f("{} RiB", &[&format!("{number:.2}")]),
            Prefix::Quetta => i18n_f("{} QiB", &[&format!("{number:.2}")]),
        }
    }
}

pub fn convert_speed(bytes_per_second: f64, network: bool) -> String {
    match SETTINGS.base() {
        Base::Decimal => {
            if network && SETTINGS.network_bits() {
                convert_speed_bits_decimal(bytes_per_second * 8.0)
            } else {
                convert_speed_decimal(bytes_per_second)
            }
        }
        Base::Binary => {
            if network && SETTINGS.network_bits() {
                convert_speed_bits_binary(bytes_per_second * 8.0)
            } else {
                convert_speed_binary(bytes_per_second)
            }
        }
    }
}

pub fn convert_speed_decimal(bytes_per_second: f64) -> String {
    let (number, prefix) = to_largest_prefix(bytes_per_second, Base::Decimal);
    match prefix {
        Prefix::None => i18n_f("{} B/s", &[&format!("{}", number.round())]),
        Prefix::Kilo => i18n_f("{} kB/s", &[&format!("{number:.2}")]),
        Prefix::Mega => i18n_f("{} MB/s", &[&format!("{number:.2}")]),
        Prefix::Giga => i18n_f("{} GB/s", &[&format!("{number:.2}")]),
        Prefix::Tera => i18n_f("{} TB/s", &[&format!("{number:.2}")]),
        Prefix::Peta => i18n_f("{} PB/s", &[&format!("{number:.2}")]),
        Prefix::Exa => i18n_f("{} EB/s", &[&format!("{number:.2}")]),
        Prefix::Zetta => i18n_f("{} ZB/s", &[&format!("{number:.2}")]),
        Prefix::Yotta => i18n_f("{} YB/s", &[&format!("{number:.2}")]),
        Prefix::Ronna => i18n_f("{} RB/s", &[&format!("{number:.2}")]),
        Prefix::Quetta => i18n_f("{} QB/s", &[&format!("{number:.2}")]),
    }
}

pub fn convert_speed_binary(bytes_per_second: f64) -> String {
    let (number, prefix) = to_largest_prefix(bytes_per_second, Base::Binary);
    match prefix {
        Prefix::None => i18n_f("{} B/s", &[&format!("{}", number.round())]),
        Prefix::Kilo => i18n_f("{} KiB/s", &[&format!("{number:.2}")]),
        Prefix::Mega => i18n_f("{} MiB/s", &[&format!("{number:.2}")]),
        Prefix::Giga => i18n_f("{} GiB/s", &[&format!("{number:.2}")]),
        Prefix::Tera => i18n_f("{} TiB/s", &[&format!("{number:.2}")]),
        Prefix::Peta => i18n_f("{} PiB/s", &[&format!("{number:.2}")]),
        Prefix::Exa => i18n_f("{} EiB/s", &[&format!("{number:.2}")]),
        Prefix::Zetta => i18n_f("{} ZiB/s", &[&format!("{number:.2}")]),
        Prefix::Yotta => i18n_f("{} YiB/s", &[&format!("{number:.2}")]),
        Prefix::Ronna => i18n_f("{} RiB/s", &[&format!("{number:.2}")]),
        Prefix::Quetta => i18n_f("{} QiB/s", &[&format!("{number:.2}")]),
    }
}

pub fn convert_speed_bits_decimal(bits_per_second: f64) -> String {
    convert_speed_bits_decimal_with_places(bits_per_second, 2)
}

pub fn convert_speed_bits_decimal_with_places(
    bits_per_second: f64,
    decimal_places: usize,
) -> String {
    let (number, prefix) = to_largest_prefix(bits_per_second, Base::Decimal);
    match prefix {
        Prefix::None => i18n_f("{} b/s", &[&format!("{}", number.round())]),
        Prefix::Kilo => i18n_f("{} kb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Mega => i18n_f("{} Mb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Giga => i18n_f("{} Gb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Tera => i18n_f("{} Tb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Peta => i18n_f("{} Pb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Exa => i18n_f("{} Eb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Zetta => i18n_f("{} Zb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Yotta => i18n_f("{} Yb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Ronna => i18n_f("{} Rb/s", &[&format!("{number:.decimal_places$}")]),
        Prefix::Quetta => i18n_f("{} Qb/s", &[&format!("{number:.decimal_places$}")]),
    }
}

pub fn convert_speed_bits_binary(bits_per_second: f64) -> String {
    let (number, prefix) = to_largest_prefix(bits_per_second, Base::Binary);
    match prefix {
        Prefix::None => i18n_f("{} b/s", &[&format!("{}", number.round())]),
        Prefix::Kilo => i18n_f("{} Kib/s", &[&format!("{number:.2}")]),
        Prefix::Mega => i18n_f("{} Mib/s", &[&format!("{number:.2}")]),
        Prefix::Giga => i18n_f("{} Gib/s", &[&format!("{number:.2}")]),
        Prefix::Tera => i18n_f("{} Tib/s", &[&format!("{number:.2}")]),
        Prefix::Peta => i18n_f("{} Pib/s", &[&format!("{number:.2}")]),
        Prefix::Exa => i18n_f("{} Eib/s", &[&format!("{number:.2}")]),
        Prefix::Zetta => i18n_f("{} Zib/s", &[&format!("{number:.2}")]),
        Prefix::Yotta => i18n_f("{} Yib/s", &[&format!("{number:.2}")]),
        Prefix::Ronna => i18n_f("{} Rib/s", &[&format!("{number:.2}")]),
        Prefix::Quetta => i18n_f("{} Qib/s", &[&format!("{number:.2}")]),
    }
}

pub fn convert_frequency(hertz: f64) -> String {
    let (number, prefix) = to_largest_prefix(hertz, Base::Decimal);
    match prefix {
        Prefix::None => i18n_f("{} Hz", &[&format!("{number:.2}")]),
        Prefix::Kilo => i18n_f("{} kHz", &[&format!("{number:.2}")]),
        Prefix::Mega => i18n_f("{} MHz", &[&format!("{number:.2}")]),
        Prefix::Giga => i18n_f("{} GHz", &[&format!("{number:.2}")]),
        Prefix::Tera => i18n_f("{} THz", &[&format!("{number:.2}")]),
        Prefix::Peta => i18n_f("{} PHz", &[&format!("{number:.2}")]),
        Prefix::Exa => i18n_f("{} EHz", &[&format!("{number:.2}")]),
        Prefix::Zetta => i18n_f("{} ZHz", &[&format!("{number:.2}")]),
        Prefix::Yotta => i18n_f("{} YHz", &[&format!("{number:.2}")]),
        Prefix::Ronna => i18n_f("{} RHz", &[&format!("{number:.2}")]),
        Prefix::Quetta => i18n_f("{} QHz", &[&format!("{number:.2}")]),
    }
}

pub fn convert_power(watts: f64) -> String {
    let (number, prefix) = to_largest_prefix(watts, Base::Decimal);
    match prefix {
        Prefix::None => i18n_f("{} W", &[&format!("{number:.1}")]),
        Prefix::Kilo => i18n_f("{} kW", &[&format!("{number:.2}")]),
        Prefix::Mega => i18n_f("{} MW", &[&format!("{number:.2}")]),
        Prefix::Giga => i18n_f("{} GW", &[&format!("{number:.2}")]),
        Prefix::Tera => i18n_f("{} TW", &[&format!("{number:.2}")]),
        Prefix::Peta => i18n_f("{} PW", &[&format!("{number:.2}")]),
        Prefix::Exa => i18n_f("{} EW", &[&format!("{number:.2}")]),
        Prefix::Zetta => i18n_f("{} ZW", &[&format!("{number:.2}")]),
        Prefix::Yotta => i18n_f("{} YW", &[&format!("{number:.2}")]),
        Prefix::Ronna => i18n_f("{} RW", &[&format!("{number:.2}")]),
        Prefix::Quetta => i18n_f("{} QW", &[&format!("{number:.2}")]),
    }
}

pub fn convert_energy(watthours: f64, integer: bool) -> String {
    let (mut number, prefix) = to_largest_prefix(watthours, Base::Decimal);
    if integer {
        number = number.round();
        match prefix {
            Prefix::None => i18n_f("{} Wh", &[&number.to_string()]),
            Prefix::Kilo => i18n_f("{} kWh", &[&number.to_string()]),
            Prefix::Mega => i18n_f("{} MWh", &[&number.to_string()]),
            Prefix::Giga => i18n_f("{} GWh", &[&number.to_string()]),
            Prefix::Tera => i18n_f("{} TWh", &[&number.to_string()]),
            Prefix::Peta => i18n_f("{} PWh", &[&number.to_string()]),
            Prefix::Exa => i18n_f("{} EWh", &[&number.to_string()]),
            Prefix::Zetta => i18n_f("{} ZWh", &[&number.to_string()]),
            Prefix::Yotta => i18n_f("{} YWh", &[&number.to_string()]),
            Prefix::Ronna => i18n_f("{} RWh", &[&number.to_string()]),
            Prefix::Quetta => i18n_f("{} QWh", &[&number.to_string()]),
        }
    } else {
        match prefix {
            Prefix::None => i18n_f("{} Wh", &[&format!("{number:.1}")]),
            Prefix::Kilo => i18n_f("{} kWh", &[&format!("{number:.2}")]),
            Prefix::Mega => i18n_f("{} MWh", &[&format!("{number:.2}")]),
            Prefix::Giga => i18n_f("{} GWh", &[&format!("{number:.2}")]),
            Prefix::Tera => i18n_f("{} TWh", &[&format!("{number:.2}")]),
            Prefix::Peta => i18n_f("{} PWh", &[&format!("{number:.2}")]),
            Prefix::Exa => i18n_f("{} EWh", &[&format!("{number:.2}")]),
            Prefix::Zetta => i18n_f("{} ZWh", &[&format!("{number:.2}")]),
            Prefix::Yotta => i18n_f("{} YWh", &[&format!("{number:.2}")]),
            Prefix::Ronna => i18n_f("{} RWh", &[&format!("{number:.2}")]),
            Prefix::Quetta => i18n_f("{} QWh", &[&format!("{number:.2}")]),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::utils::{
        settings::Base,
        units::{Prefix, celsius_to_fahrenheit, celsius_to_kelvin, to_largest_prefix},
    };
    use pretty_assertions::assert_eq;

    use super::format_time;

    #[test]
    fn format_time_negative() {
        let seconds = -3723.13;
        let formatted_time = format_time(seconds);
        assert_eq!("-1∶02∶03.13", formatted_time)
    }

    #[test]
    fn format_time_zero() {
        let seconds = 0.0;
        let formatted_time = format_time(seconds);
        assert_eq!("0∶00∶00.00", formatted_time)
    }

    #[test]
    fn format_time_positive() {
        let seconds = 3723.13;
        let formatted_time = format_time(seconds);
        assert_eq!("1∶02∶03.13", formatted_time)
    }

    #[test]
    fn format_time_nan() {
        let seconds = f64::NAN;
        let formatted_time = format_time(seconds);
        assert_eq!("NaN", formatted_time)
    }

    #[test]
    fn format_time_infinity() {
        let seconds = f64::INFINITY;
        let formatted_time = format_time(seconds);
        assert_eq!("∞", formatted_time)
    }

    #[test]
    fn format_time_neg_infinity() {
        let seconds = f64::NEG_INFINITY;
        let formatted_time = format_time(seconds);
        assert_eq!("-∞", formatted_time)
    }

    #[test]
    fn to_largest_prefix_decimal_giga_negative() {
        let raw = -123_400_000_000.0;
        let formatted = to_largest_prefix(raw, Base::Decimal);
        assert_eq!((-123.4f64, Prefix::Giga), formatted)
    }

    #[test]
    fn to_largest_prefix_binary_giga_negative() {
        let raw = -132_499_741_081.6;
        let formatted = to_largest_prefix(raw, Base::Binary);
        assert_eq!((-123.4f64, Prefix::Giga), formatted)
    }

    #[test]
    fn to_largest_prefix_decimal_none() {
        let raw = 123.4;
        let formatted = to_largest_prefix(raw, Base::Decimal);
        assert_eq!((123.4f64, Prefix::None), formatted)
    }

    #[test]
    fn to_largest_prefix_binary_none() {
        let raw = 123.4;
        let formatted = to_largest_prefix(raw, Base::Binary);
        assert_eq!((123.4f64, Prefix::None), formatted)
    }

    #[test]
    fn to_largest_prefix_decimal_giga() {
        let raw = 123_400_000_000.0;
        let formatted = to_largest_prefix(raw, Base::Decimal);
        assert_eq!((123.4f64, Prefix::Giga), formatted)
    }

    #[test]
    fn to_largest_prefix_binary_giga() {
        let raw = 132_499_741_081.6;
        let formatted = to_largest_prefix(raw, Base::Binary);
        assert_eq!((123.4f64, Prefix::Giga), formatted)
    }

    #[test]
    fn to_largest_prefix_nan() {
        let raw = f64::NAN;
        let formatted = to_largest_prefix(raw, Base::Binary);
        // normal assert_eq! is not possible because NaN != NaN
        assert_eq!(formatted.0.is_nan(), true);
        assert_eq!(formatted.1, Prefix::None);
    }

    #[test]
    fn to_largest_prefix_infinity() {
        let raw = f64::INFINITY;
        let formatted = to_largest_prefix(raw, Base::Binary);
        assert_eq!((f64::INFINITY, Prefix::None), formatted)
    }

    #[test]
    fn to_largest_prefix_neg_infinity() {
        let raw = f64::NEG_INFINITY;
        let formatted = to_largest_prefix(raw, Base::Binary);
        assert_eq!((f64::NEG_INFINITY, Prefix::None), formatted)
    }

    #[test]
    fn celsius_to_kelvin_valid() {
        let celsius = 20.0;
        let kelvin = celsius_to_kelvin(celsius);
        assert_eq!(293.15, kelvin);
    }

    #[test]
    fn celsius_to_fahrenheit_valid() {
        let celsius = 20.0;
        let fahrenheit = celsius_to_fahrenheit(celsius);
        assert_eq!(68.0, fahrenheit);
    }
}
