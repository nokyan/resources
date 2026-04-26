use adw::{ActionRow, prelude::ActionRowExt};

use crate::{
    i18n::{i18n, i18n_f},
    utils::units::{convert_fraction, convert_storage, convert_temperature},
};

extern crate pastey;

#[macro_export]
macro_rules! gstring_getter_setter {
    ($($gstring_name:ident),*) => {
        $(
            pub fn $gstring_name(&self) -> glib::GString {
                let $gstring_name = self.$gstring_name.take();
                self.$gstring_name.set($gstring_name.clone());
                $gstring_name
            }

            pastey::paste! {
                pub fn [<set_ $gstring_name>](&self, $gstring_name: &str) {
                    self.$gstring_name.set(glib::GString::from($gstring_name));
                }
            }
        )*
    };
}

#[macro_export]
macro_rules! gstring_option_getter_setter {
    ($($gstring_name:ident),*) => {
        $(
            pub fn $gstring_name(&self) -> Option<glib::GString> {
                let $gstring_name = self.$gstring_name.take();
                self.$gstring_name.set($gstring_name.clone());
                $gstring_name
            }

            pastey::paste! {
                pub fn [<set_ $gstring_name>](&self, $gstring_name: Option<&str>) {
                    self.$gstring_name.set($gstring_name.map(glib::GString::from));
                }
            }
        )*
    };
}

fn set_subtitle_maybe<S: AsRef<str>>(subtitle: Option<S>, action_row: &ActionRow) {
    if let Some(subtitle) = subtitle {
        action_row.set_subtitle(subtitle.as_ref());
    } else {
        action_row.set_subtitle(&i18n("N/A"));
    }
}

fn set_subtitle_converted_maybe<T, S: AsRef<str>, F: Fn(T) -> S>(
    value: Option<T>,
    stringify_fn: F,
    action_row: &ActionRow,
) {
    set_subtitle_maybe(value.map(|v| stringify_fn(v)), action_row);
}

fn set_subtitle_boolean(boolean: bool, action_row: &ActionRow) -> String {
    let subtitle = if boolean { i18n("Yes") } else { i18n("No") };

    action_row.set_subtitle(&subtitle);

    subtitle
}

fn set_subtitle_boolean_maybe(boolean: Option<bool>, action_row: &ActionRow) -> String {
    if let Some(boolean) = boolean {
        set_subtitle_boolean(boolean, action_row)
    } else {
        action_row.set_subtitle(&i18n("N/A"));
        i18n("N/A")
    }
}

fn gpu_npu_usage_string(
    usage_fraction: Option<f64>,
    used_memory: Option<u64>,
    total_memory: Option<u64>,
    temperature: Option<f64>,
) -> String {
    let mut elements = Vec::with_capacity(3);

    if let Some(usage_fraction) = usage_fraction {
        elements.push(convert_fraction(usage_fraction, true));
    }

    if let (Some(used_memory), Some(total_memory)) = (used_memory, total_memory) {
        elements.push(i18n_f(
            // Translators: This will be displayed in the sidebar, please try to keep your translation as short as (or even
            // shorter than) 'Memory'
            "Memory: {}",
            &[&convert_fraction(
                used_memory as f64 / total_memory as f64,
                true,
            )],
        ));
    } else if let Some(used_memory) = used_memory {
        elements.push(i18n_f(
            // Translators: This will be displayed in the sidebar, please try to keep your translation as short as (or even
            // shorter than) 'Memory'
            "Memory: {}",
            &[&convert_storage(used_memory as f64, true)],
        ));
    }

    if let Some(temperature) = temperature {
        elements.push(convert_temperature(temperature));
    }

    elements.join(" · ").to_string()
}

pub mod dialogs;
pub mod pages;
pub mod widgets;
pub mod window;
