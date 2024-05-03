use std::ffi::OsString;

use crate::application;
#[rustfmt::skip]
use crate::config;
use crate::utils::app::DATA_DIRS;
use crate::utils::IS_FLATPAK;

use gettextrs::{gettext, LocaleCategory};
use gtk::{gio, glib};

use self::application::Application;
use self::config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};

pub fn main() {
    // Initialize logger
    pretty_env_logger::init();

    // reset XDG_DATA_DIRS to use absolute paths instead of relative paths because Flatpak seemingly cannot resolve them
    // this must happen now because once the GTK app is loaded, it's too late
    if *IS_FLATPAK {
        std::env::set_var(
            "XDG_DATA_DIRS",
            DATA_DIRS
                .iter()
                .map(|pathbuf| pathbuf.as_os_str().to_owned())
                .collect::<Vec<OsString>>()
                .join(&OsString::from(":")),
        );
    }

    // Prepare i18n
    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain");
    gettextrs::textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    glib::set_application_name(&gettext("Resources"));

    let res = gio::Resource::load(RESOURCES_FILE).expect("Could not load gresource file");
    gio::resources_register(&res);

    let app = Application::new();
    app.run();
}
