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
    #[template(resource = "/me/nalux/Resources/ui/dialogs/settings_dialog.ui")]
    pub struct ResSettingsDialog {
        #[template_child]
        pub prefix_combo_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub temperature_combo_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub refresh_speed_combo_row: TemplateChild<adw::ComboRow>,
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
        imp.temperature_combo_row
            .set_selected((SETTINGS.temperature_unit() as u8) as u32);
        imp.refresh_speed_combo_row
            .set_selected((SETTINGS.refresh_speed() as u8) as u32);
    }

    pub fn setup_signals(&self) {
        let imp = self.imp();
        imp.prefix_combo_row
            .connect_selected_item_notify(|combo_row| {
                if let Some(base) = Base::from_repr(combo_row.selected() as u8) {
                    let _ = SETTINGS.set_base(base);
                }
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
    }
}
