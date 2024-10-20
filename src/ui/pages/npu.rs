use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self};

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::utils::npu::{Npu, NpuData};
use crate::utils::units::{convert_frequency, convert_power, convert_storage, convert_temperature};
use crate::utils::FiniteOr;

pub const TAB_ID_PREFIX: &str = "npu";

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::{pages::NPU_PRIMARY_ORD, widgets::graph_box::ResGraphBox};

    use super::*;

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/npu.ui")]
    #[properties(wrapper_type = super::ResNPU)]
    pub struct ResNPU {
        #[template_child]
        pub npu_usage: TemplateChild<ResGraphBox>,
        #[template_child]
        pub memory_usage: TemplateChild<ResGraphBox>,
        #[template_child]
        pub temperature: TemplateChild<ResGraphBox>,
        #[template_child]
        pub power_usage: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub npu_clockspeed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub memory_clockspeed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub manufacturer: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub pci_slot: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub driver_used: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub max_power_cap: TemplateChild<adw::ActionRow>,

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

    impl ResNPU {
        gstring_getter_setter!(tab_name, tab_detail_string, tab_usage_string, tab_id);
    }

    impl Default for ResNPU {
        fn default() -> Self {
            Self {
                npu_usage: Default::default(),
                memory_usage: Default::default(),
                temperature: Default::default(),
                power_usage: Default::default(),
                npu_clockspeed: Default::default(),
                memory_clockspeed: Default::default(),
                manufacturer: Default::default(),
                pci_slot: Default::default(),
                driver_used: Default::default(),
                max_power_cap: Default::default(),
                uses_progress_bar: Cell::new(true),
                main_graph_color: glib::Bytes::from_static(&super::ResNPU::MAIN_GRAPH_COLOR),
                icon: RefCell::new(ThemedIcon::new("npu-symbolic").into()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("NPU"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_usage_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::new()),
                graph_locked_max_y: Cell::new(true),
                primary_ord: Cell::new(NPU_PRIMARY_ORD),
                secondary_ord: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResNPU {
        const NAME: &'static str = "ResNPU";
        type Type = super::ResNPU;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResNPU {
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

    impl WidgetImpl for ResNPU {}
    impl BinImpl for ResNPU {}
}

glib::wrapper! {
    pub struct ResNPU(ObjectSubclass<imp::ResNPU>)
        @extends gtk::Widget, adw::Bin;
}

impl Default for ResNPU {
    fn default() -> Self {
        Self::new()
    }
}

impl ResNPU {
    const MAIN_GRAPH_COLOR: [u8; 3] = [0xb5, 0x27, 0xe3];

    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, npu: &Npu, secondary_ord: u32) {
        self.set_secondary_ord(secondary_ord);
        self.setup_widgets(npu);
    }

    pub fn setup_widgets(&self, npu: &Npu) {
        let imp = self.imp();

        let tab_id = format!("{}-{}", TAB_ID_PREFIX, &npu.pci_slot().to_string());
        imp.set_tab_id(&tab_id);

        imp.npu_usage.set_title_label(&i18n("Total Usage"));
        imp.npu_usage.graph().set_graph_color(
            Self::MAIN_GRAPH_COLOR[0],
            Self::MAIN_GRAPH_COLOR[1],
            Self::MAIN_GRAPH_COLOR[2],
        );

        imp.memory_usage.set_title_label(&i18n("Memory Usage"));
        imp.memory_usage.graph().set_graph_color(0x9e, 0x1c, 0xcc);

        imp.temperature.set_title_label(&i18n("Temperature"));
        imp.temperature.graph().set_graph_color(0x83, 0x1c, 0xac);
        imp.temperature.graph().set_locked_max_y(None);

        imp.manufacturer.set_subtitle(
            &npu.get_vendor()
                .map_or_else(|_| i18n("N/A"), |vendor| vendor.name().to_string()),
        );

        imp.pci_slot.set_subtitle(&npu.pci_slot().to_string());

        imp.driver_used.set_subtitle(&npu.driver());

        if let Ok(model_name) = npu.name() {
            imp.set_tab_detail_string(&model_name);
        }
    }

    pub fn refresh_page(&self, npu_data: &NpuData) {
        let imp = self.imp();

        let NpuData {
            pci_slot: _,
            usage_fraction,
            total_memory,
            used_memory,
            clock_speed,
            vram_speed,
            temperature,
            power_usage,
            power_cap,
            power_cap_max,
        } = npu_data;

        let mut usage_percentage_string = usage_fraction.map_or_else(
            || i18n("N/A"),
            |fraction| format!("{} %", (fraction * 100.0).round()),
        );

        imp.npu_usage.set_subtitle(&usage_percentage_string);
        imp.npu_usage
            .graph()
            .push_data_point(usage_fraction.unwrap_or(0.0));
        imp.npu_usage.graph().set_visible(usage_fraction.is_some());

        let used_memory_fraction =
            if let (Some(total_memory), Some(used_memory)) = (total_memory, used_memory) {
                Some((*used_memory as f64 / *total_memory as f64).finite_or_default())
            } else {
                None
            };

        let memory_percentage_string = used_memory_fraction.as_ref().map_or_else(
            || i18n("N/A"),
            |fraction| format!("{} %", (fraction * 100.0).round()),
        );

        let memory_subtitle =
            if let (Some(total_memory), Some(used_memory)) = (total_memory, used_memory) {
                format!(
                    "{} / {} · {}",
                    convert_storage(*used_memory as f64, false),
                    convert_storage(*total_memory as f64, false),
                    memory_percentage_string
                )
            } else {
                i18n("N/A")
            };

        imp.memory_usage.set_subtitle(&memory_subtitle);
        imp.memory_usage
            .graph()
            .push_data_point(used_memory_fraction.unwrap_or(0.0));
        imp.memory_usage
            .graph()
            .set_visible(used_memory_fraction.is_some());

        imp.temperature.graph().set_visible(temperature.is_some());

        let mut power_string = power_usage.map_or_else(|| i18n("N/A"), convert_power);

        if let Some(power_cap) = power_cap {
            power_string.push_str(&format!(" / {}", convert_power(*power_cap)));
        }

        imp.power_usage.set_subtitle(&power_string);

        if let Some(npu_clockspeed) = clock_speed {
            imp.npu_clockspeed
                .set_subtitle(&convert_frequency(*npu_clockspeed));
        } else {
            imp.npu_clockspeed.set_subtitle(&i18n("N/A"));
        }

        if let Some(vram_clockspeed) = vram_speed {
            imp.memory_clockspeed
                .set_subtitle(&convert_frequency(*vram_clockspeed));
        } else {
            imp.memory_clockspeed.set_subtitle(&i18n("N/A"));
        }

        imp.max_power_cap
            .set_subtitle(&power_cap_max.map_or_else(|| i18n("N/A"), convert_power));

        self.set_property("usage", usage_fraction.unwrap_or(0.0));

        if used_memory_fraction.is_some() {
            usage_percentage_string.push_str(" · ");
            // Translators: This will be displayed in the sidebar, please try to keep your translation as short as (or even
            // shorter than) 'Memory'
            usage_percentage_string.push_str(&i18n_f("Memory: {}", &[&memory_percentage_string]));
        }

        if let Some(temperature) = temperature {
            let temperature_string = convert_temperature(*temperature as f64);

            let highest_temperature_string =
                convert_temperature(imp.temperature.graph().get_highest_value());

            imp.temperature.set_subtitle(&format!(
                "{} · {} {}",
                &temperature_string,
                i18n("Highest:"),
                highest_temperature_string
            ));
            imp.temperature.graph().push_data_point(*temperature as f64);

            usage_percentage_string.push_str(" · ");
            usage_percentage_string.push_str(&temperature_string);
        } else {
            imp.temperature.set_subtitle(&i18n("N/A"));
        }

        self.set_property("tab_usage_string", &usage_percentage_string);
    }
}
