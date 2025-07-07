use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self};
use log::trace;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::utils::FiniteOr;
use crate::utils::gpu::{Gpu, GpuData};
use crate::utils::units::{convert_frequency, convert_power, convert_storage, convert_temperature};

pub const TAB_ID_PREFIX: &str = "gpu";

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::{
        pages::GPU_PRIMARY_ORD,
        widgets::{double_graph_box::ResDoubleGraphBox, graph_box::ResGraphBox},
    };

    use super::*;

    use gtk::{
        CompositeTemplate,
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/gpu.ui")]
    #[properties(wrapper_type = super::ResGPU)]
    pub struct ResGPU {
        #[template_child]
        pub gpu_usage: TemplateChild<ResGraphBox>,
        #[template_child]
        pub encode_decode_usage: TemplateChild<ResDoubleGraphBox>,
        #[template_child]
        pub encode_decode_combined_usage: TemplateChild<ResGraphBox>,
        #[template_child]
        pub vram_usage: TemplateChild<ResGraphBox>,
        #[template_child]
        pub temperature: TemplateChild<ResGraphBox>,
        #[template_child]
        pub power_usage: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub gpu_clockspeed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub vram_clockspeed: TemplateChild<adw::ActionRow>,
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

    impl ResGPU {
        gstring_getter_setter!(tab_name, tab_detail_string, tab_usage_string, tab_id);
    }

    impl Default for ResGPU {
        fn default() -> Self {
            Self {
                gpu_usage: Default::default(),
                encode_decode_usage: Default::default(),
                encode_decode_combined_usage: Default::default(),
                vram_usage: Default::default(),
                temperature: Default::default(),
                power_usage: Default::default(),
                gpu_clockspeed: Default::default(),
                vram_clockspeed: Default::default(),
                manufacturer: Default::default(),
                pci_slot: Default::default(),
                driver_used: Default::default(),
                max_power_cap: Default::default(),
                link: Default::default(),
                uses_progress_bar: Cell::new(true),
                main_graph_color: glib::Bytes::from_static(&super::ResGPU::MAIN_GRAPH_COLOR),
                icon: RefCell::new(ThemedIcon::new("gpu-symbolic").into()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("GPU"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_usage_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::new()),
                graph_locked_max_y: Cell::new(true),
                primary_ord: Cell::new(GPU_PRIMARY_ORD),
                secondary_ord: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResGPU {
        const NAME: &'static str = "ResGPU";
        type Type = super::ResGPU;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResGPU {
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

    impl WidgetImpl for ResGPU {}
    impl BinImpl for ResGPU {}
}

glib::wrapper! {
    pub struct ResGPU(ObjectSubclass<imp::ResGPU>)
        @extends gtk::Widget, adw::Bin;
}

impl Default for ResGPU {
    fn default() -> Self {
        Self::new()
    }
}

impl ResGPU {
    const MAIN_GRAPH_COLOR: [u8; 3] = [0xed, 0x33, 0x3b];

    pub fn new() -> Self {
        trace!("Creating ResGPU GObject…");

        glib::Object::new::<Self>()
    }

    pub fn init(&self, gpu: &Gpu, secondary_ord: u32) {
        self.set_secondary_ord(secondary_ord);
        self.setup_widgets(gpu);
    }

    pub fn setup_widgets(&self, gpu: &Gpu) {
        trace!(
            "Setting up ResGPU ({}, {}) widgets…",
            self.tab_detail_string(),
            gpu.gpu_identifier()
        );

        let imp = self.imp();

        let tab_id = format!("{}-{}", TAB_ID_PREFIX, &gpu.gpu_identifier());
        imp.set_tab_id(&tab_id);

        imp.gpu_usage.set_title_label(&i18n("Total Usage"));
        imp.gpu_usage.graph().set_graph_color(
            Self::MAIN_GRAPH_COLOR[0],
            Self::MAIN_GRAPH_COLOR[1],
            Self::MAIN_GRAPH_COLOR[2],
        );

        imp.encode_decode_combined_usage
            .set_title_label(&i18n("Video Encoder/Decoder Usage"));
        imp.encode_decode_combined_usage
            .graph()
            .set_graph_color(0xe0, 0x1b, 0x24);

        imp.encode_decode_usage
            .set_start_title_label(&i18n("Video Encoder Usage"));
        imp.encode_decode_usage
            .start_graph()
            .set_graph_color(0xe0, 0x1b, 0x24);
        imp.encode_decode_usage
            .set_end_title_label(&i18n("Video Decoder Usage"));
        imp.encode_decode_usage
            .end_graph()
            .set_graph_color(0xe0, 0x1b, 0x24);

        imp.vram_usage.set_title_label(&i18n("Video Memory Usage"));
        imp.vram_usage.graph().set_graph_color(0xc0, 0x1c, 0x28);

        imp.temperature.set_title_label(&i18n("Temperature"));
        imp.temperature.graph().set_graph_color(0xa5, 0x1d, 0x2d);
        imp.temperature.graph().set_locked_max_y(None);

        imp.manufacturer.set_subtitle(
            &gpu.get_vendor()
                .map_or_else(|_| i18n("N/A"), |vendor| vendor.name().to_string()),
        );

        match gpu.gpu_identifier() {
            process_data::GpuIdentifier::PciSlot(pci_slot) => {
                imp.pci_slot.set_subtitle(&pci_slot.to_string());
            }
            process_data::GpuIdentifier::Enumerator(_) => imp.pci_slot.set_subtitle(&i18n("N/A")),
        }

        imp.driver_used.set_subtitle(gpu.driver());

        if gpu.combined_media_engine().unwrap_or_default() {
            imp.encode_decode_combined_usage.set_visible(true);
            imp.encode_decode_usage.set_visible(false);
        } else {
            imp.encode_decode_combined_usage.set_visible(false);
            imp.encode_decode_usage.set_visible(true);
        }

        if let Ok(model_name) = gpu.name() {
            imp.set_tab_detail_string(&model_name);
        }
    }

    pub fn refresh_page(&self, gpu_data: &GpuData) {
        trace!(
            "Refreshing ResGPU ({}, {})…",
            self.tab_detail_string(),
            gpu_data.gpu_identifier
        );

        let imp = self.imp();

        let GpuData {
            gpu_identifier: _,
            usage_fraction,
            encode_fraction,
            decode_fraction,
            total_vram,
            used_vram,
            clock_speed,
            vram_speed,
            temperature,
            power_usage,
            power_cap,
            power_cap_max,
            link,
            nvidia: _,
        } = gpu_data;

        let mut usage_percentage_string = usage_fraction.map_or_else(
            || i18n("N/A"),
            |fraction| format!("{} %", (fraction * 100.0).round()),
        );

        imp.gpu_usage.set_subtitle(&usage_percentage_string);
        imp.gpu_usage
            .graph()
            .push_data_point(usage_fraction.unwrap_or(0.0));
        imp.gpu_usage.graph().set_visible(usage_fraction.is_some());

        // encode_fraction could be the combined usage of encoder and decoder for Intel GPUs and newer AMD GPUs
        if let Some(encode_fraction) = encode_fraction {
            imp.encode_decode_usage
                .start_graph()
                .push_data_point(*encode_fraction);
            imp.encode_decode_usage
                .set_start_subtitle(&format!("{} %", (encode_fraction * 100.0).round()));

            imp.encode_decode_combined_usage
                .graph()
                .push_data_point(*encode_fraction);
            imp.encode_decode_combined_usage
                .set_subtitle(&format!("{} %", (encode_fraction * 100.0).round()));
        } else {
            imp.encode_decode_usage.start_graph().push_data_point(0.0);
            imp.encode_decode_usage.set_start_subtitle(&i18n("N/A"));

            imp.encode_decode_combined_usage.graph().set_visible(false);
            imp.encode_decode_combined_usage.set_subtitle(&i18n("N/A"));
        }

        if let Some(decode_fraction) = decode_fraction {
            imp.encode_decode_usage
                .end_graph()
                .push_data_point(*decode_fraction);
            imp.encode_decode_usage
                .set_end_subtitle(&format!("{} %", (decode_fraction * 100.0).round()));
        } else {
            imp.encode_decode_usage.end_graph().push_data_point(0.0);
            imp.encode_decode_usage.set_end_subtitle(&i18n("N/A"));
        }

        // only turn enc and dec graphs invisible if both of them are None, otherwise only one of them might show a
        // graph and that'd look odd
        imp.encode_decode_usage
            .start_graph()
            .set_visible(encode_fraction.is_some() || decode_fraction.is_some());
        imp.encode_decode_usage
            .end_graph()
            .set_visible(encode_fraction.is_some() || decode_fraction.is_some());

        let used_vram_fraction =
            if let (Some(total_vram), Some(used_vram)) = (total_vram, used_vram) {
                Some((*used_vram as f64 / *total_vram as f64).finite_or_default())
            } else {
                None
            };

        let vram_percentage_string = used_vram_fraction.as_ref().map_or_else(
            || i18n("N/A"),
            |fraction| format!("{} %", (fraction * 100.0).round()),
        );

        let vram_subtitle = if let (Some(total_vram), Some(used_vram)) = (total_vram, used_vram) {
            format!(
                "{} / {} · {}",
                convert_storage(*used_vram as f64, false),
                convert_storage(*total_vram as f64, false),
                vram_percentage_string
            )
        } else {
            i18n("N/A")
        };

        imp.vram_usage.set_subtitle(&vram_subtitle);
        imp.vram_usage
            .graph()
            .push_data_point(used_vram_fraction.unwrap_or(0.0));
        imp.vram_usage
            .graph()
            .set_visible(used_vram_fraction.is_some());

        let mut power_string = power_usage.map_or_else(|| i18n("N/A"), convert_power);

        if let Some(power_cap) = power_cap {
            power_string.push_str(&format!(" / {}", convert_power(*power_cap)));
        }

        imp.power_usage.set_subtitle(&power_string);

        if let Some(gpu_clockspeed) = clock_speed {
            imp.gpu_clockspeed
                .set_subtitle(&convert_frequency(*gpu_clockspeed));
        } else {
            imp.gpu_clockspeed.set_subtitle(&i18n("N/A"));
        }

        if let Some(vram_clockspeed) = vram_speed {
            imp.vram_clockspeed
                .set_subtitle(&convert_frequency(*vram_clockspeed));
        } else {
            imp.vram_clockspeed.set_subtitle(&i18n("N/A"));
        }

        imp.max_power_cap
            .set_subtitle(&power_cap_max.map_or_else(|| i18n("N/A"), convert_power));

        self.set_property("usage", usage_fraction.unwrap_or(0.0));

        if used_vram_fraction.is_some() {
            usage_percentage_string.push_str(" · ");
            // Translators: This will be displayed in the sidebar, please try to keep your translation as short as (or even
            // shorter than) 'Memory'
            usage_percentage_string.push_str(&i18n_f("Memory: {}", &[&vram_percentage_string]));
        }

        imp.temperature.graph().set_visible(temperature.is_some());

        if let Some(temperature) = temperature {
            let temperature_string = convert_temperature(*temperature);

            let highest_temperature_string =
                convert_temperature(imp.temperature.graph().get_highest_value());

            imp.temperature.set_subtitle(&format!(
                "{} · {} {}",
                &temperature_string,
                i18n("Highest:"),
                highest_temperature_string
            ));
            imp.temperature.graph().push_data_point(*temperature);

            usage_percentage_string.push_str(" · ");
            usage_percentage_string.push_str(&temperature_string);
        } else {
            imp.temperature.set_subtitle(&i18n("N/A"));
        }

        if let Some(link) = link {
            imp.link.set_subtitle(&link.to_string());
        } else {
            imp.link.set_subtitle(&i18n("N/A"));
        }

        self.set_property("tab_usage_string", &usage_percentage_string);
    }
}
