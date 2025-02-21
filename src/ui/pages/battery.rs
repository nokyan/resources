use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;
use log::trace;

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::utils::battery::BatteryData;
use crate::utils::units::{convert_energy, convert_power};

pub const TAB_ID_PREFIX: &str = "battery";

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::{pages::BATTERY_PRIMARY_ORD, widgets::graph_box::ResGraphBox};

    use super::*;

    use gtk::{
        CompositeTemplate,
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/battery.ui")]
    #[properties(wrapper_type = super::ResBattery)]
    pub struct ResBattery {
        #[template_child]
        pub charge: TemplateChild<ResGraphBox>,
        #[template_child]
        pub power_usage: TemplateChild<ResGraphBox>,
        #[template_child]
        pub health: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub design_capacity: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub charge_cycles: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub technology: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub manufacturer: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub model_name: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub device: TemplateChild<adw::ActionRow>,

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get)]
        main_graph_color: glib::Bytes,

        #[property(get)]
        icon: RefCell<Icon>,

        #[property(get, set)]
        usage: Cell<f64>,

        #[property(get = Self::tab_name, set = Self::set_tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,

        #[property(get = Self::tab_detail_string, set = Self::set_tab_detail_string, type = glib::GString)]
        tab_detail_string: Cell<glib::GString>,

        #[property(get = Self::tab_usage_string, set = Self::set_tab_usage_string, type = glib::GString)]
        tab_usage_string: Cell<glib::GString>,

        #[property(get = Self::tab_id, set = Self::set_tab_id, type = glib::GString)]
        tab_id: Cell<glib::GString>,

        #[property(get)]
        graph_locked_max_y: Cell<bool>,

        #[property(get)]
        primary_ord: Cell<u32>,

        #[property(get, set)]
        secondary_ord: Cell<u32>,
    }

    impl ResBattery {
        gstring_getter_setter!(tab_name, tab_detail_string, tab_usage_string, tab_id);
    }

    impl Default for ResBattery {
        fn default() -> Self {
            Self {
                charge: Default::default(),
                power_usage: Default::default(),
                health: Default::default(),
                design_capacity: Default::default(),
                charge_cycles: Default::default(),
                technology: Default::default(),
                manufacturer: Default::default(),
                model_name: Default::default(),
                device: Default::default(),
                uses_progress_bar: Cell::new(true),
                main_graph_color: glib::Bytes::from_static(&super::ResBattery::MAIN_GRAPH_COLOR),
                icon: RefCell::new(ThemedIcon::new("battery-symbolic").into()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("Drive"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::new()),
                tab_usage_string: Cell::new(glib::GString::new()),
                graph_locked_max_y: Cell::new(true),
                primary_ord: Cell::new(BATTERY_PRIMARY_ORD),
                secondary_ord: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResBattery {
        const NAME: &'static str = "ResBattery";
        type Type = super::ResBattery;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResBattery {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }

        fn properties() -> &'static [ParamSpec] {
            Self::derived_properties()
        }

        fn set_property(&self, id: usize, value: &Value, pspec: &ParamSpec) {
            self.derived_set_property(id, value, pspec);
        }

        fn property(&self, id: usize, pspec: &ParamSpec) -> Value {
            self.derived_property(id, pspec)
        }
    }

    impl WidgetImpl for ResBattery {}
    impl BinImpl for ResBattery {}
}

glib::wrapper! {
    pub struct ResBattery(ObjectSubclass<imp::ResBattery>)
        @extends gtk::Widget, adw::Bin;
}

impl Default for ResBattery {
    fn default() -> Self {
        Self::new()
    }
}

impl ResBattery {
    const MAIN_GRAPH_COLOR: [u8; 3] = [0x33, 0xd1, 0x7a];

    pub fn new() -> Self {
        trace!("Creating ResBattery GObject…");

        glib::Object::new::<Self>()
    }

    pub fn init(&self, battery_data: &BatteryData, secondary_ord: u32) {
        self.set_secondary_ord(secondary_ord);
        self.setup_widgets(battery_data);
    }

    pub fn setup_widgets(&self, battery_data: &BatteryData) {
        trace!(
            "Setting up ResBattery ({}) widgets…",
            battery_data.inner.sysfs_path.to_string_lossy()
        );

        let imp = self.imp();
        let battery = &battery_data.inner;

        let tab_id = format!(
            "{}-{}-{}-{}",
            TAB_ID_PREFIX,
            battery.manufacturer.as_deref().unwrap_or_default(),
            battery.model_name.as_deref().unwrap_or_default(),
            battery.sysfs_path.file_name().unwrap().to_string_lossy(),
        );
        imp.set_tab_id(&tab_id);

        if let Some(design_capacity) = battery.design_capacity {
            let converted_energy = convert_energy(design_capacity, false);
            imp.design_capacity.set_subtitle(&converted_energy);
        } else {
            imp.design_capacity.set_subtitle(&i18n("N/A"));
        }

        imp.set_tab_name(&battery.display_name());

        imp.charge.set_title_label(&i18n("Battery Charge"));
        imp.charge.graph().set_graph_color(
            Self::MAIN_GRAPH_COLOR[0],
            Self::MAIN_GRAPH_COLOR[1],
            Self::MAIN_GRAPH_COLOR[2],
        );

        imp.power_usage.set_title_label(&i18n("Power Usage"));
        imp.power_usage.graph().set_graph_color(0x26, 0xa2, 0x69);
        imp.power_usage.graph().set_locked_max_y(None);

        imp.charge_cycles.set_subtitle(
            &battery_data
                .charge_cycles
                .as_ref()
                .map_or_else(|_| i18n("N/A"), std::string::ToString::to_string),
        );

        imp.technology.set_subtitle(&battery.technology.to_string());

        imp.manufacturer
            .set_subtitle(&battery.manufacturer.clone().unwrap_or_else(|| i18n("N/A")));

        imp.model_name
            .set_subtitle(&battery.model_name.clone().unwrap_or_else(|| i18n("N/A")));

        imp.device
            .set_subtitle(&battery.sysfs_path.file_name().unwrap().to_string_lossy());

        imp.set_tab_detail_string(&battery.sysfs_path.file_name().unwrap().to_string_lossy());
    }

    pub fn refresh_page(&self, battery_data: BatteryData) {
        trace!(
            "Refreshing ResBattery ({})…",
            battery_data.inner.sysfs_path.to_string_lossy()
        );

        let imp = self.imp();

        let mut usage_string = String::new();

        if let Ok(charge) = battery_data.charge {
            let mut percentage_string = format!("{} %", (charge * 100.0).round());
            usage_string.push_str(&percentage_string);

            if let Ok(state) = battery_data.state {
                percentage_string.push_str(&format!(" ({state})"));
            }

            imp.charge.graph().set_visible(true);
            imp.charge.graph().push_data_point(charge);
            imp.charge.set_subtitle(&percentage_string);
        } else {
            imp.charge.graph().set_visible(false);
            imp.charge.set_subtitle(&i18n("N/A"));
        }
        self.set_property("usage", battery_data.charge.unwrap_or_default());

        if let Ok(power_usage) = battery_data.power_usage {
            imp.power_usage.graph().push_data_point(power_usage);

            let formatted_power = convert_power(power_usage);
            let formatted_highest_power =
                convert_power(imp.power_usage.graph().get_highest_value());

            imp.power_usage.graph().set_visible(true);
            imp.power_usage.set_subtitle(&format!(
                "{formatted_power} · {} {formatted_highest_power}",
                i18n("Highest:")
            ));

            if !usage_string.is_empty() {
                usage_string.push_str(" · ");
            }

            usage_string.push_str(&formatted_power);
        } else {
            imp.power_usage.graph().set_visible(false);
            imp.power_usage.set_subtitle(&i18n("N/A"));
            if usage_string.is_empty() {
                usage_string.push_str(&i18n("N/A"));
            }
        }

        self.set_tab_usage_string(usage_string);

        if let Ok(health) = battery_data.health {
            imp.health
                .set_subtitle(&format!("{} %", (health * 100.0).round()));
        } else {
            imp.health.set_subtitle(&i18n("N/A"));
        }
    }
}
