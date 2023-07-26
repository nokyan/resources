use adw::{prelude::*, subclass::prelude::*};
use anyhow::Context;
use gtk::glib::{self, clone};

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::utils::memory::{
    self, get_available_memory, get_free_swap, get_total_memory, get_total_swap, MemoryDevice,
};
use crate::utils::units::{to_largest_unit, Base};
use crate::utils::NaNDefault;

mod imp {
    use crate::ui::widgets::{graph_box::ResGraphBox, info_box::ResInfoBox};

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/pages/memory.ui")]
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
        pub slots_used: TemplateChild<ResInfoBox>,
        #[template_child]
        pub speed: TemplateChild<ResInfoBox>,
        #[template_child]
        pub form_factor: TemplateChild<ResInfoBox>,
        #[template_child]
        pub memory_type: TemplateChild<ResInfoBox>,
        #[template_child]
        pub type_detail: TemplateChild<ResInfoBox>,
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
    }

    impl WidgetImpl for ResMemory {}
    impl BinImpl for ResMemory {}
}

glib::wrapper! {
    pub struct ResMemory(ObjectSubclass<imp::ResMemory>)
        @extends gtk::Widget, adw::Bin;
}

impl ResMemory {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self) {
        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();
        imp.memory.set_title_label(&i18n("Memory"));
        imp.memory.set_graph_color(129, 61, 156);
        imp.memory.set_data_points_max_amount(60);
        imp.swap.set_title_label(&i18n("Swap"));
        imp.swap.set_graph_color(46, 194, 126);
        imp.swap.set_data_points_max_amount(60);

        if let Ok(memory_devices) = memory::get_memory_devices() {
            self.setup_properties(memory_devices);
        } else {
            imp.properties.set_visible(false);
            imp.authentication_banner.set_revealed(true)
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
            .map(|md| md.speed.unwrap_or(0))
            .max()
            .unwrap_or(0);
        let form_factor = memory_devices
            .iter()
            .filter(|md| md.installed)
            .nth(0)
            .map(|md| md.form_factor.clone())
            .unwrap_or(i18n("N/A"));
        let r#type = memory_devices
            .iter()
            .filter(|md| md.installed)
            .nth(0)
            .map(|md| md.r#type.clone())
            .unwrap_or(i18n("N/A"));
        let type_detail = memory_devices
            .iter()
            .filter(|md| md.installed)
            .nth(0)
            .map(|md| md.type_detail.clone())
            .unwrap_or(i18n("N/A"));
        imp.slots_used
            .set_info_label(&i18n_f("{} of {}", &[slots_used.as_str(), slots.as_str()]));
        imp.speed.set_info_label(&format!("{} MT/s", speed));
        imp.form_factor.set_info_label(&form_factor);
        imp.memory_type.set_info_label(&r#type);
        imp.type_detail.set_info_label(&type_detail);
    }

    pub fn setup_signals(&self) {
        let imp = self.imp();
        imp.authentication_banner
            .connect_button_clicked(clone!(@strong self as this => move |_| {
                let imp = this.imp();
                if let Ok(memory_devices) = memory::pkexec_get_memory_devices() {
                    this.setup_properties(memory_devices);
                    imp.properties.set_visible(true);
                }
                imp.authentication_banner.set_revealed(false)
            }));
        let mem_usage_update = clone!(@strong self as this => move || {
            let imp = this.imp();
            let total_mem = get_total_memory().with_context(|| "unable to get total memory").unwrap_or_default();
            let available_mem = get_available_memory().with_context(|| "unable to get available memory").unwrap_or_default();
            let total_swap = get_total_swap().with_context(|| "unable to get total swap").unwrap_or_default();
            let free_swap = get_free_swap().with_context(|| "unable to get free swap").unwrap_or_default();

            let total_mem_unit = to_largest_unit(total_mem as f64, &Base::Decimal);
            let used_mem_unit = to_largest_unit((total_mem - available_mem) as f64, &Base::Decimal);
            let total_swap_unit = to_largest_unit(total_swap as f64, &Base::Decimal);
            let used_swap_unit = to_largest_unit((total_swap - free_swap) as f64, &Base::Decimal);

            let memory_fraction = 1.0 - (available_mem as f64 / total_mem as f64);
            let swap_fraction = 1.0 - (free_swap as f64 / total_swap as f64).nan_default(1.0);

            imp.memory.push_data_point(memory_fraction);
            imp.memory.set_info_label(&format!("{:.2} {}B / {:.2} {}B · {} %", used_mem_unit.0, used_mem_unit.1, total_mem_unit.0, total_mem_unit.1, (memory_fraction * 100.0) as u8));
            if total_swap == 0 {
                imp.swap.push_data_point(0.0);
                imp.swap.set_graph_visible(false);
                imp.swap.set_info_label(&i18n("N/A"));
            } else {
                imp.swap.push_data_point(swap_fraction);
                imp.swap.set_graph_visible(true);
                imp.swap.set_info_label(&format!("{:.2} {}B / {:.2} {}B · {} %", used_swap_unit.0, used_swap_unit.1, total_swap_unit.0, total_swap_unit.1, (swap_fraction * 100.0) as u8));
            }

            glib::Continue(true)
        });

        glib::timeout_add_seconds_local(1, mem_usage_update);
    }
}
