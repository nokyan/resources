use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self};

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::utils::gpu::{Gpu, GpuData};
use crate::utils::units::{convert_frequency, convert_power, convert_storage, convert_temperature};
use crate::utils::NaNDefault;

mod imp {
    use std::{
        cell::{Cell, RefCell},
        sync::OnceLock,
    };

    use crate::ui::widgets::{double_graph_box::ResDoubleGraphBox, graph_box::ResGraphBox};

    use super::*;

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        CompositeTemplate,
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
        pub temperature: TemplateChild<adw::ActionRow>,
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
        pub current_power_cap: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub max_power_cap: TemplateChild<adw::ActionRow>,

        pub number: OnceLock<usize>,

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

        #[property(get = Self::tab_detail, set = Self::set_tab_detail, type = glib::GString)]
        tab_detail_string: Cell<glib::GString>,

        #[property(get = Self::tab_usage_string, set = Self::set_tab_usage_string, type = glib::GString)]
        tab_usage_string: Cell<glib::GString>,
    }

    impl ResGPU {
        pub fn tab_name(&self) -> glib::GString {
            let tab_name = self.tab_name.take();
            let result = tab_name.clone();
            self.tab_name.set(tab_name);
            result
        }

        pub fn set_tab_name(&self, tab_name: &str) {
            self.tab_name.set(glib::GString::from(tab_name));
        }

        pub fn tab_detail(&self) -> glib::GString {
            let detail = self.tab_detail_string.take();
            let result = detail.clone();
            self.tab_detail_string.set(detail);
            result
        }

        pub fn set_tab_detail(&self, detail: &str) {
            self.tab_detail_string.set(glib::GString::from(detail));
        }

        pub fn tab_usage_string(&self) -> glib::GString {
            let tab_usage_string = self.tab_usage_string.take();
            let result = tab_usage_string.clone();
            self.tab_usage_string.set(tab_usage_string);
            result
        }

        pub fn set_tab_usage_string(&self, tab_usage_string: &str) {
            self.tab_usage_string
                .set(glib::GString::from(tab_usage_string));
        }
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
                current_power_cap: Default::default(),
                max_power_cap: Default::default(),
                number: Default::default(),
                uses_progress_bar: Cell::new(true),
                main_graph_color: glib::Bytes::from_static(&super::ResGPU::MAIN_GRAPH_COLOR),
                icon: RefCell::new(ThemedIcon::new("gpu-symbolic").into()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("GPU"))),
                tab_detail_string: Cell::new(glib::GString::from("")),
                tab_usage_string: Cell::new(glib::GString::from("")),
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

impl ResGPU {
    const MAIN_GRAPH_COLOR: [u8; 3] = [230, 97, 0];

    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, gpu: &Gpu, number: usize) {
        let imp = self.imp();
        imp.number.set(number).unwrap_or_default();
        self.setup_widgets(gpu);
    }

    pub fn setup_widgets(&self, gpu: &Gpu) {
        let imp = self.imp();

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
            .set_graph_color(230, 97, 0);

        imp.encode_decode_usage
            .set_start_title_label(&i18n("Video Encoder Usage"));
        imp.encode_decode_usage
            .start_graph()
            .set_graph_color(211, 49, 14);
        imp.encode_decode_usage
            .set_end_title_label(&i18n("Video Decoder Usage"));
        imp.encode_decode_usage
            .end_graph()
            .set_graph_color(211, 49, 14);

        imp.vram_usage.set_title_label(&i18n("Video Memory Usage"));
        imp.vram_usage.graph().set_graph_color(192, 28, 40);

        imp.manufacturer.set_subtitle(
            &gpu.get_vendor()
                .map(|vendor| vendor.name().to_string())
                .unwrap_or_else(|_| i18n("N/A")),
        );

        imp.pci_slot.set_subtitle(&gpu.pci_slot().to_string());

        imp.driver_used.set_subtitle(&gpu.driver());

        if gpu
            .get_vendor()
            .map(|vendor| vendor.vid())
            .unwrap_or_default()
            == crate::utils::gpu::VID_INTEL
        {
            imp.encode_decode_combined_usage.set_visible(true);
            imp.encode_decode_usage.set_visible(false);
        } else {
            imp.encode_decode_combined_usage.set_visible(false);
            imp.encode_decode_usage.set_visible(true);
        }

        if let Ok(model_name) = gpu.name() {
            imp.set_tab_detail(&model_name);
        }
    }

    pub fn refresh_page(&self, gpu_data: GpuData) {
        let imp = self.imp();

        let GpuData {
            pci_slot: _,
            usage_fraction,
            encode_fraction,
            decode_fraction,
            total_vram,
            used_vram,
            clock_speed,
            vram_speed,
            temp,
            power_usage,
            power_cap,
            power_cap_max,
        } = gpu_data;

        let mut usage_percentage_string = usage_fraction
            .map(|fraction| format!("{} %", (fraction * 100.0).round()))
            .unwrap_or(i18n("N/A"));

        imp.gpu_usage.set_subtitle(&usage_percentage_string);
        imp.gpu_usage
            .graph()
            .push_data_point(usage_fraction.unwrap_or(0.0));
        imp.gpu_usage.graph().set_visible(usage_fraction.is_some());

        // encode_fraction could be the combined usage of encoder and decoder for intel GPUs
        if let Some(encode_fraction) = encode_fraction {
            imp.encode_decode_usage
                .start_graph()
                .push_data_point(encode_fraction);
            imp.encode_decode_usage
                .set_start_subtitle(&format!("{} %", (encode_fraction * 100.0).round()));

            imp.encode_decode_combined_usage
                .graph()
                .push_data_point(encode_fraction);
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
                .push_data_point(decode_fraction);
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
                Some((used_vram as f64 / total_vram as f64).nan_default(0.0))
            } else {
                None
            };

        let vram_percentage_string = used_vram_fraction
            .as_ref()
            .map(|fraction| format!("{} %", (fraction * 100.0).round()))
            .unwrap_or(i18n("N/A"));

        let vram_subtitle = if let (Some(total_vram), Some(used_vram)) = (total_vram, used_vram) {
            format!(
                "{} / {} · {}",
                convert_storage(used_vram as f64, false),
                convert_storage(total_vram as f64, false),
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

        let temperature_string = temp.map(convert_temperature);

        imp.temperature
            .set_subtitle(&temperature_string.clone().unwrap_or_else(|| i18n("N/A")));

        imp.power_usage
            .set_subtitle(&power_usage.map_or_else(|| i18n("N/A"), convert_power));

        if let Some(gpu_clockspeed) = clock_speed {
            imp.gpu_clockspeed
                .set_subtitle(&convert_frequency(gpu_clockspeed));
        } else {
            imp.gpu_clockspeed.set_subtitle(&i18n("N/A"));
        }

        if let Some(vram_clockspeed) = vram_speed {
            imp.vram_clockspeed
                .set_subtitle(&convert_frequency(vram_clockspeed));
        } else {
            imp.vram_clockspeed.set_subtitle(&i18n("N/A"));
        }

        imp.current_power_cap
            .set_subtitle(&power_cap.map_or_else(|| i18n("N/A"), convert_power));

        imp.max_power_cap
            .set_subtitle(&power_cap_max.map_or_else(|| i18n("N/A"), convert_power));

        self.set_property("usage", usage_fraction.unwrap_or(0.0));

        if used_vram_fraction.is_some() {
            usage_percentage_string.push_str(" · ");
            usage_percentage_string.push_str(&i18n_f("VRAM: {}", &[&vram_percentage_string]));
        }

        if let Some(temperature_string) = temperature_string {
            usage_percentage_string.push_str(" · ");
            usage_percentage_string.push_str(&temperature_string);
        }

        self.set_property("tab_usage_string", &usage_percentage_string)
    }
}
