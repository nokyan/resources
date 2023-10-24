use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::{
    config::PROFILE,
    utils::settings::{Base, RefreshSpeed, TemperatureUnit, SETTINGS},
};

mod imp {

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/net/nokyan/Resources/ui/dialogs/settings_dialog.ui")]
    pub struct ResSettingsDialog {
        #[template_child]
        pub prefix_combo_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub network_bits_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub temperature_combo_row: TemplateChild<adw::ComboRow>,

        #[template_child]
        pub refresh_speed_combo_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub show_search_on_start_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub sidebar_details_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub apps_show_memory_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub apps_show_cpu_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub apps_show_drive_read_speed_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub apps_show_drive_read_total_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub apps_show_drive_write_speed_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub apps_show_drive_write_total_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub processes_show_id_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub processes_show_user_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub processes_show_memory_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub processes_show_cpu_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub processes_show_drive_read_speed_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub processes_show_drive_read_total_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub processes_show_drive_write_speed_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub processes_show_drive_write_total_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub show_virtual_drives_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub show_virtual_network_interfaces_row: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResSettingsDialog {
        const NAME: &'static str = "ResSettingsDialog";
        type Type = super::ResSettingsDialog;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResSettingsDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResSettingsDialog {}

    impl WindowImpl for ResSettingsDialog {}

    impl AdwWindowImpl for ResSettingsDialog {}

    impl PreferencesWindowImpl for ResSettingsDialog {}
}

glib::wrapper! {
    pub struct ResSettingsDialog(ObjectSubclass<imp::ResSettingsDialog>)
        @extends adw::PreferencesWindow, gtk::Window, gtk::Widget, adw::Window;
}

impl ResSettingsDialog {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self) {
        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();
        imp.prefix_combo_row
            .set_selected((SETTINGS.base() as u8) as u32);
        imp.network_bits_row.set_active(SETTINGS.network_bits());
        imp.temperature_combo_row
            .set_selected((SETTINGS.temperature_unit() as u8) as u32);

        imp.refresh_speed_combo_row
            .set_selected((SETTINGS.refresh_speed() as u8) as u32);
        imp.sidebar_details_row
            .set_active(SETTINGS.sidebar_details());
        imp.show_search_on_start_row
            .set_active(SETTINGS.show_search_on_start());

        imp.apps_show_memory_row
            .set_active(SETTINGS.apps_show_memory());
        imp.apps_show_cpu_row.set_active(SETTINGS.apps_show_cpu());
        imp.apps_show_drive_read_speed_row
            .set_active(SETTINGS.apps_show_drive_read_speed());
        imp.apps_show_drive_read_total_row
            .set_active(SETTINGS.apps_show_drive_read_total());
        imp.apps_show_drive_write_speed_row
            .set_active(SETTINGS.apps_show_drive_write_speed());
        imp.apps_show_drive_write_total_row
            .set_active(SETTINGS.apps_show_drive_write_total());

        imp.processes_show_id_row
            .set_active(SETTINGS.processes_show_id());
        imp.processes_show_user_row
            .set_active(SETTINGS.processes_show_user());
        imp.processes_show_memory_row
            .set_active(SETTINGS.processes_show_memory());
        imp.processes_show_cpu_row
            .set_active(SETTINGS.processes_show_cpu());
        imp.processes_show_drive_read_speed_row
            .set_active(SETTINGS.processes_show_drive_read_speed());
        imp.processes_show_drive_read_total_row
            .set_active(SETTINGS.processes_show_drive_read_total());
        imp.processes_show_drive_write_speed_row
            .set_active(SETTINGS.processes_show_drive_write_speed());
        imp.processes_show_drive_write_total_row
            .set_active(SETTINGS.processes_show_drive_write_total());

        imp.show_virtual_drives_row
            .set_active(SETTINGS.show_virtual_drives());
        imp.show_virtual_network_interfaces_row
            .set_active(SETTINGS.show_virtual_network_interfaces());
    }

    pub fn setup_signals(&self) {
        let imp = self.imp();
        imp.prefix_combo_row
            .connect_selected_item_notify(|combo_row| {
                if let Some(base) = Base::from_repr(combo_row.selected() as u8) {
                    let _ = SETTINGS.set_base(base);
                }
            });

        imp.network_bits_row.connect_active_notify(|switch_row| {
            let _ = SETTINGS.set_network_bits(switch_row.is_active());
        });

        imp.temperature_combo_row
            .connect_selected_item_notify(|combo_row| {
                if let Some(temperature_unit) =
                    TemperatureUnit::from_repr(combo_row.selected() as u8)
                {
                    let _ = SETTINGS.set_temperature_unit(temperature_unit);
                }
            });

        imp.refresh_speed_combo_row
            .connect_selected_item_notify(|combo_row| {
                if let Some(refresh_speed) = RefreshSpeed::from_repr(combo_row.selected() as u8) {
                    let _ = SETTINGS.set_refresh_speed(refresh_speed);
                }
            });

        imp.sidebar_details_row.connect_active_notify(|switch_row| {
            let _ = SETTINGS.set_sidebar_details(switch_row.is_active());
        });

        imp.show_search_on_start_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_show_search_on_start(switch_row.is_active());
            });

        imp.apps_show_cpu_row.connect_active_notify(|switch_row| {
            let _ = SETTINGS.set_apps_show_cpu(switch_row.is_active());
        });

        imp.apps_show_memory_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_apps_show_memory(switch_row.is_active());
            });

        imp.apps_show_drive_read_speed_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_apps_show_drive_read_speed(switch_row.is_active());
            });

        imp.apps_show_drive_read_total_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_apps_show_drive_read_total(switch_row.is_active());
            });

        imp.apps_show_drive_write_speed_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_apps_show_drive_write_speed(switch_row.is_active());
            });

        imp.apps_show_drive_write_total_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_apps_show_drive_write_total(switch_row.is_active());
            });

        imp.processes_show_id_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_processes_show_id(switch_row.is_active());
            });

        imp.processes_show_user_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_processes_show_user(switch_row.is_active());
            });

        imp.processes_show_cpu_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_processes_show_cpu(switch_row.is_active());
            });

        imp.processes_show_memory_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_processes_show_memory(switch_row.is_active());
            });

        imp.processes_show_drive_read_speed_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_processes_show_drive_read_speed(switch_row.is_active());
            });

        imp.processes_show_drive_read_total_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_processes_show_drive_read_total(switch_row.is_active());
            });

        imp.processes_show_drive_write_speed_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_processes_show_drive_write_speed(switch_row.is_active());
            });

        imp.processes_show_drive_write_total_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_processes_show_drive_write_total(switch_row.is_active());
            });

        imp.show_virtual_drives_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_show_virtual_drives(switch_row.is_active());
            });

        imp.show_virtual_network_interfaces_row
            .connect_active_notify(|switch_row| {
                let _ = SETTINGS.set_show_virtual_network_interfaces(switch_row.is_active());
            });
    }
}
