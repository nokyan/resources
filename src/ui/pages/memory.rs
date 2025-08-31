use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone};
use log::trace;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::utils::FiniteOr;
use crate::utils::memory::{MemoryData, MemoryDevice};
use crate::utils::units::convert_storage;

pub const TAB_ID: &str = "memory";

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::{pages::MEMORY_PRIMARY_ORD, widgets::graph_box::ResGraphBox};

    use super::*;

    use gtk::{
        CompositeTemplate,
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/memory.ui")]
    #[properties(wrapper_type = super::ResMemory)]
    pub struct ResMemory {
        #[template_child]
        pub memory: TemplateChild<ResGraphBox>,
        #[template_child]
        pub swap: TemplateChild<ResGraphBox>,
        #[template_child]
        pub authentication_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub properties: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub slots_used: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub speed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub form_factor: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub memory_type: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub type_detail: TemplateChild<adw::ActionRow>,

        pub memory_devices: RefCell<Vec<MemoryDevice>>,

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get)]
        main_graph_color: glib::Bytes,

        #[property(get)]
        icon: RefCell<Icon>,

        #[property(get, set)]
        usage: Cell<f64>,

        #[property(get = Self::tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,

        #[property(get = Self::tab_detail_string, set = Self::set_tab_detail_string, type = glib::GString)]
        tab_detail_string: Cell<glib::GString>,

        #[property(get = Self::tab_usage_string, set = Self::set_tab_usage_string, type = glib::GString)]
        tab_usage_string: Cell<glib::GString>,

        #[property(get = Self::tab_id, type = glib::GString)]
        tab_id: Cell<glib::GString>,

        #[property(get)]
        graph_locked_max_y: Cell<bool>,

        #[property(get)]
        primary_ord: Cell<u32>,

        #[property(get)]
        secondary_ord: Cell<u32>,
    }

    impl ResMemory {
        gstring_getter_setter!(tab_name, tab_detail_string, tab_usage_string, tab_id);
    }

    impl Default for ResMemory {
        fn default() -> Self {
            Self {
                memory: Default::default(),
                swap: Default::default(),
                authentication_banner: Default::default(),
                properties: Default::default(),
                slots_used: Default::default(),
                speed: Default::default(),
                form_factor: Default::default(),
                memory_type: Default::default(),
                type_detail: Default::default(),
                memory_devices: Default::default(),
                uses_progress_bar: Cell::new(true),
                main_graph_color: glib::Bytes::from_static(&super::ResMemory::MAIN_GRAPH_COLOR),
                icon: RefCell::new(ThemedIcon::new("memory-symbolic").into()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("Memory"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_usage_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::from(TAB_ID)),
                graph_locked_max_y: Cell::new(true),
                primary_ord: Cell::new(MEMORY_PRIMARY_ORD),
                secondary_ord: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResMemory {
        const NAME: &'static str = "ResMemory";
        type Type = super::ResMemory;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResMemory {
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

    impl WidgetImpl for ResMemory {}
    impl BinImpl for ResMemory {}
}

glib::wrapper! {
    pub struct ResMemory(ObjectSubclass<imp::ResMemory>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Accessible;
}

impl Default for ResMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl ResMemory {
    const MAIN_GRAPH_COLOR: [u8; 3] = [0xc5, 0x2f, 0x90];

    pub fn new() -> Self {
        trace!("Creating ResMemory GObject…");

        glib::Object::new::<Self>()
    }

    pub fn init(&self) {
        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        trace!("Setting up ResMemory widgets…");

        let imp = self.imp();

        imp.memory.set_title_label(&i18n("Memory"));
        imp.memory.graph().set_graph_color(
            Self::MAIN_GRAPH_COLOR[0],
            Self::MAIN_GRAPH_COLOR[1],
            Self::MAIN_GRAPH_COLOR[2],
        );
        imp.swap.set_title_label(&i18n("Swap"));
        imp.swap.graph().set_graph_color(0x94, 0x29, 0x7c);

        if let Ok(memory_devices) = MemoryDevice::get() {
            self.setup_properties(memory_devices);
        } else {
            imp.properties.set_visible(false);
            imp.authentication_banner.set_revealed(true);
        }
    }

    pub fn setup_properties(&self, memory_devices: Vec<MemoryDevice>) {
        let imp = self.imp();

        let slots_used = memory_devices
            .iter()
            .filter(|md| md.installed)
            .count()
            .to_string();

        let slots = memory_devices.len().to_string();

        let speed = memory_devices
            .iter()
            .filter(|md| md.installed)
            .map(|md| md.speed_mts.unwrap_or(0))
            .max();

        let form_factor = memory_devices.iter().find(|md| md.installed).map_or_else(
            || i18n("N/A"),
            |md| md.form_factor.clone().unwrap_or_else(|| i18n("N/A")),
        );

        let r#type = memory_devices.iter().find(|md| md.installed).map_or_else(
            || i18n("N/A"),
            |md| md.r#type.clone().unwrap_or_else(|| i18n("N/A")),
        );

        let type_detail = memory_devices.iter().find(|md| md.installed).map_or_else(
            || i18n("N/A"),
            |md| md.type_detail.clone().unwrap_or_else(|| i18n("N/A")),
        );

        let total_memory = memory_devices
            .iter()
            .map(|md| md.size.unwrap_or(0))
            .sum::<u64>() as f64;

        self.set_property(
            "tab_detail_string",
            format!(
                "{} {}",
                convert_storage(total_memory, false),
                memory_devices
                    .iter()
                    .find(|md| md.installed)
                    .and_then(|md| md.r#type.clone())
                    .unwrap_or_default()
            ),
        );

        imp.memory_devices.replace(memory_devices);

        imp.slots_used
            .set_subtitle(&i18n_f("{} of {}", &[slots_used.as_str(), slots.as_str()]));

        imp.speed.set_subtitle(&i18n_f(
            "{} MT/s",
            &[&speed.map_or_else(|| i18n("N/A"), |i| i.to_string())],
        ));

        imp.form_factor.set_subtitle(&form_factor);

        imp.memory_type.set_subtitle(&r#type);

        imp.type_detail.set_subtitle(&type_detail);
    }

    pub fn setup_signals(&self) {
        trace!("Setting up ResMemory signals…");

        let imp = self.imp();

        imp.authentication_banner.connect_button_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let imp = this.imp();
                if let Ok(memory_devices) = MemoryDevice::pkexec_dmidecode() {
                    this.setup_properties(memory_devices);
                    imp.properties.set_visible(true);
                }
                imp.authentication_banner.set_revealed(false);
            }
        ));
    }

    pub fn refresh_page(&self, memdata: MemoryData) {
        trace!("Refreshing ResMemory…");

        let imp = self.imp();

        let MemoryData {
            total_mem,
            available_mem,
            total_swap,
            free_swap,
        } = memdata;

        let used_mem = total_mem.saturating_sub(available_mem);
        let used_swap = total_swap.saturating_sub(free_swap);

        let memory_fraction = used_mem as f64 / total_mem as f64;
        let swap_fraction = (used_swap as f64 / total_swap as f64).finite_or_default();

        let formatted_used_mem = convert_storage(used_mem as f64, false);
        let formatted_total_mem = convert_storage(total_mem as f64, false);

        imp.memory.graph().push_data_point(memory_fraction);
        imp.memory.set_subtitle(&format!(
            "{} / {} · {} %",
            &formatted_used_mem,
            &formatted_total_mem,
            (memory_fraction * 100.0).round()
        ));
        if total_swap == 0 {
            // no swap detected
            imp.swap.graph().push_data_point(0.0);
            imp.swap.graph().set_visible(false);
            imp.swap.set_subtitle(&i18n("N/A"));
            self.set_property(
                "tab_usage_string",
                format!("{} / {}", &formatted_used_mem, &formatted_total_mem),
            );
        } else {
            imp.swap.graph().push_data_point(swap_fraction);
            imp.swap.graph().set_visible(true);
            imp.swap.set_subtitle(&format!(
                "{} / {} · {} %",
                &convert_storage(used_swap as f64, false),
                &convert_storage(total_swap as f64, false),
                (swap_fraction * 100.0).round()
            ));
            self.set_property(
                "tab_usage_string",
                i18n_f(
                    // Translators: This will be displayed in the sidebar, so your translation for "Swap" should
                    // preferably be quite short or an abbreviation
                    "{} / {} · Swap: {} %",
                    &[
                        &formatted_used_mem,
                        &formatted_total_mem,
                        &(swap_fraction * 100.0).round().to_string(),
                    ],
                ),
            );
        }

        let memory_devices = imp.memory_devices.borrow();

        let total_memory = memory_devices
            .iter()
            .map(|md| md.size.unwrap_or(0))
            .sum::<u64>() as f64;

        self.set_property(
            "tab_detail_string",
            format!(
                "{} {}",
                convert_storage(total_memory, false),
                memory_devices
                    .iter()
                    .find(|md| md.installed)
                    .and_then(|md| md.r#type.clone())
                    .unwrap_or_default()
            ),
        );

        self.set_property("usage", memory_fraction);
    }
}
