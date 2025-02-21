use std::ffi::OsString;
use std::sync::LazyLock;

use crate::application;
#[rustfmt::skip]
use crate::config;
use crate::utils::IS_FLATPAK;
use crate::utils::app::DATA_DIRS;

use clap::{Parser, command};
use gettextrs::{LocaleCategory, gettext};
use gtk::{gio, glib};
use log::trace;

use self::application::Application;
use self::config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};

pub static ARGS: LazyLock<Args> = LazyLock::new(Args::parse);

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Args {
    /// Disable GPU monitoring
    #[arg(short = 'g', long, default_value_t = false)]
    pub disable_gpu_monitoring: bool,

    /// Disable network interface monitoring
    #[arg(short = 'n', long, default_value_t = false)]
    pub disable_network_interface_monitoring: bool,

    /// Disable drive monitoring
    #[arg(short = 'd', long, default_value_t = false)]
    pub disable_drive_monitoring: bool,

    /// Disable battery monitoring
    #[arg(short = 'b', long, default_value_t = false)]
    pub disable_battery_monitoring: bool,

    /// Disable CPU monitoring
    #[arg(short = 'c', long, default_value_t = false)]
    pub disable_cpu_monitoring: bool,

    /// Disable memory monitoring
    #[arg(short = 'm', long, default_value_t = false)]
    pub disable_memory_monitoring: bool,

    /// Disable NPU monitoring
    #[arg(short = 'v', long, default_value_t = false)]
    pub disable_npu_monitoring: bool,

    /// Disable process monitoring
    #[arg(short = 'p', long, default_value_t = false)]
    pub disable_process_monitoring: bool,

    /// Open tab specified by ID.
    /// Valid IDs are: "applications", "processes", "cpu", "memory", "gpu-$PCI_SLOT$",
    /// "drive-$MODEL_NAME_OR_DEVICE_NAME$", "network-$INTERFACE_NAME$",
    /// "battery-$MANUFACTURER$-$MODEL_NAME$-$DEVICE_NAME$"
    #[arg(short = 't', long)]
    pub open_tab_id: Option<String>,
}

pub fn main() {
    // Force args parsing here so we don't start printing logs before printing the help page
    std::hint::black_box(ARGS.disable_battery_monitoring);

    // Initialize logger
    pretty_env_logger::init();
    trace!("Trace logs activated. Brace yourself for *lots* of logs. Slowdowns may occur.");

    // reset XDG_DATA_DIRS to use absolute paths instead of relative paths because Flatpak seemingly cannot resolve them
    // this must happen now because once the GTK app is loaded, it's too late
    if *IS_FLATPAK {
        unsafe {
            std::env::set_var(
                "XDG_DATA_DIRS",
                DATA_DIRS
                    .iter()
                    .map(|pathbuf| pathbuf.as_os_str().to_owned())
                    .collect::<Vec<OsString>>()
                    .join(&OsString::from(":")),
            );
        }
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
