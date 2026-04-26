use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self};
use log::trace;
use std::fmt::Write;

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::{gpu_npu_usage_string, set_subtitle_converted_maybe};
use crate::utils::link::Link;
use crate::utils::npu::{Npu, NpuData};
use crate::utils::units::{convert_frequency, convert_power, convert_tops};

pub const TAB_ID_PREFIX: &str = "npu";

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::{pages::NPU_PRIMARY_ORD, widgets::graph_box::ResGraphBox};

    use super::*;

    use gtk::{
        CompositeTemplate,
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
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
        pub tops: TemplateChild<adw::ActionRow>,
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
        #[template_child]
        pub link: TemplateChild<adw::ActionRow>,

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
                tops: Default::default(),
                memory_clockspeed: Default::default(),
                manufacturer: Default::default(),
                pci_slot: Default::default(),
                driver_used: Default::default(),
                max_power_cap: Default::default(),
                link: Default::default(),
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
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Accessible;
}

impl Default for ResNPU {
    fn default() -> Self {
        Self::new()
    }
}

impl ResNPU {
    const MAIN_GRAPH_COLOR: [u8; 3] = [0xb5, 0x27, 0xe3];

    pub fn new() -> Self {
        trace!("Creating ResNPU GObject…");

        glib::Object::new::<Self>()
    }

    pub fn init(&self, npu: &Npu, secondary_ord: u32) {
        self.set_secondary_ord(secondary_ord);
        self.setup_widgets(npu);
    }

    pub fn setup_widgets(&self, npu: &Npu) {
        trace!(
            "Setting up ResNPU ({}, {}) widgets…",
            self.tab_detail_string(),
            npu.pci_slot()
        );

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

        imp.driver_used.set_subtitle(npu.driver());

        if let Ok(model_name) = npu.name() {
            imp.set_tab_detail_string(&model_name);
        }
    }

    pub fn refresh_page(&self, npu_data: &NpuData) {
        trace!(
            "Refreshing ResNPU ({}, {})…",
            self.tab_detail_string(),
            npu_data.pci_slot
        );

        let imp = self.imp();

        let NpuData {
            pci_slot: _,
            usage_fraction,
            total_memory,
            used_memory,
            clock_speed,
            vram_speed,
            curr_tops,
            max_tops,
            temperature,
            power_usage,
            power_cap,
            power_cap_max,
            link,
        } = npu_data;

        imp.npu_usage.add_fraction_point(*usage_fraction);

        imp.memory_usage
            .add_storage_point(*used_memory, *total_memory);

        imp.temperature.add_temperature_point(*temperature);

        let mut power_string = power_usage.map_or_else(|| i18n("N/A"), convert_power);

        if let Some(power_cap) = power_cap {
            let _ = write!(power_string, " / {}", convert_power(*power_cap));
        }

        imp.power_usage.set_subtitle(&power_string);

        set_subtitle_converted_maybe(*clock_speed, convert_frequency, &imp.npu_clockspeed);

        if let (Some(curr_tops), Some(max_tops)) = (curr_tops, max_tops) {
            imp.tops.set_subtitle(&format!(
                "{} / {}",
                convert_tops(*curr_tops as f64),
                convert_tops(*max_tops as f64)
            ));
        } else {
            imp.tops.set_subtitle(&i18n("N/A"));
        }

        set_subtitle_converted_maybe(*vram_speed, convert_frequency, &imp.npu_clockspeed);

        set_subtitle_converted_maybe(*power_cap_max, convert_power, &imp.max_power_cap);

        self.set_property("usage", usage_fraction.unwrap_or(0.0));

        imp.temperature.add_temperature_point(*temperature);

        set_subtitle_converted_maybe(link.as_ref(), Link::to_string, &imp.link);

        self.set_property(
            "tab_usage_string",
            gpu_npu_usage_string(*usage_fraction, *used_memory, *total_memory, *temperature),
        );
    }
}
