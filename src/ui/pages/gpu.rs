use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone};

use crate::config::PROFILE;
use crate::ui::widgets::info_box::ResInfoBox;
use crate::ui::widgets::progress_box::ResProgressBox;
use crate::utils::gpu::GPU;
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use crate::utils::gpu::GPU;

    use super::*;

    use gtk::CompositeTemplate;
    use once_cell::sync::OnceCell;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/pages/gpu.ui")]
    pub struct ResGPU {
        pub gpu: OnceCell<GPU>,
        pub number: OnceCell<usize>,
        #[template_child]
        pub gpu_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub gpu_usage: TemplateChild<ResProgressBox>,
        #[template_child]
        pub vram_usage: TemplateChild<ResProgressBox>,
        #[template_child]
        pub temperature: TemplateChild<ResInfoBox>,
        #[template_child]
        pub power_usage: TemplateChild<ResInfoBox>,
        #[template_child]
        pub gpu_clockspeed: TemplateChild<ResInfoBox>,
        #[template_child]
        pub vram_clockspeed: TemplateChild<ResInfoBox>,
        #[template_child]
        pub manufacturer: TemplateChild<ResInfoBox>,
        #[template_child]
        pub pci_slot: TemplateChild<ResInfoBox>,
        #[template_child]
        pub driver_used: TemplateChild<ResInfoBox>,
        #[template_child]
        pub current_power_cap: TemplateChild<ResInfoBox>,
        #[template_child]
        pub max_power_cap: TemplateChild<ResInfoBox>,
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
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
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
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create ResGPU")
    }

    pub fn init(&self, gpu: GPU, number: usize) {
        let imp = self.imp();
        imp.gpu.set(gpu).unwrap_or_default();
        imp.number.set(number).unwrap_or_default();
        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();
        let gpu = &imp.gpu.get().unwrap();
        imp.gpu_name.set_label(
            &gpu.get_name()
                .unwrap_or(gettextrs::gettext!("GPU {}", imp.number.get().unwrap() + 1)),
        );
        imp.manufacturer.set_info_label(
            &gpu.get_vendor()
                .unwrap_or_else(|_| gettextrs::gettext("Unknown")),
        );
        imp.pci_slot.set_info_label(&gpu.pci_slot);
        imp.driver_used.set_info_label(&gpu.driver);
    }

    pub fn setup_signals(&self) {
        let gpu_usage_update = clone!(@strong self as this => move || {
            let imp = this.imp();
            let gpu = imp.gpu.get().unwrap();

            if let Ok(gpu_usage) = gpu.get_gpu_usage() {
                imp.gpu_usage.set_percentage_label(&format!("{} %", gpu_usage));
                imp.gpu_usage.set_fraction(gpu_usage as f64 / 100.0);
                imp.gpu_usage.set_progressbar_visible(true);
            } else {
                imp.gpu_usage.set_percentage_label(&gettextrs::gettext("N/A"));
                imp.gpu_usage.set_progressbar_visible(false);
            }

            if let (Ok(total_vram), Ok(used_vram)) = (gpu.get_total_vram(), gpu.get_used_vram()) {
                let total_vram_unit = to_largest_unit(total_vram as f64, Base::Decimal);
                let used_vram_unit = to_largest_unit(used_vram as f64, Base::Decimal);
                imp.vram_usage.set_percentage_label(&format!("{:.2} {}B / {:.2} {}B", used_vram_unit.0, used_vram_unit.1, total_vram_unit.0, total_vram_unit.1));
                imp.vram_usage.set_fraction((used_vram as f64) / (total_vram as f64));
                imp.vram_usage.set_progressbar_visible(true);
            } else {
                imp.vram_usage.set_percentage_label(&gettextrs::gettext("N/A"));
                imp.vram_usage.set_progressbar_visible(false);
            }

            // TODO: handle the user's choice of temperatue unit
            let temp_unit = "C";
            imp.temperature.set_info_label(&gpu.get_gpu_temp().map(|x| format!("{} °{}", x, temp_unit)).unwrap_or_else(|_| gettextrs::gettext("N/A")));

            imp.power_usage.set_info_label(&gpu.get_power_usage().map(|x| format!("{:.2} W", x)).unwrap_or_else(|_| gettextrs::gettext("N/A")));

            if let Ok(gpu_clockspeed) = gpu.get_gpu_speed() {
                let gpu_clockspeed_unit = to_largest_unit(gpu_clockspeed, Base::Decimal);
                imp.gpu_clockspeed.set_info_label(&format!("{:.2} {}Hz", gpu_clockspeed_unit.0, gpu_clockspeed_unit.1));
            } else {
                imp.gpu_clockspeed.set_info_label(&gettextrs::gettext("N/A"));
            }

            if let Ok(vram_clockspeed) = gpu.get_vram_speed() {
                let vram_clockspeed_unit = to_largest_unit(vram_clockspeed, Base::Decimal);
                imp.vram_clockspeed.set_info_label(&format!("{:.2} {}Hz", vram_clockspeed_unit.0, vram_clockspeed_unit.1));
            } else {
                imp.vram_clockspeed.set_info_label(&gettextrs::gettext("N/A"));
            }

            imp.current_power_cap.set_info_label(&gpu.get_power_cap().map(|x| format!("{:.2} W", x)).unwrap_or_else(|_| gettextrs::gettext("N/A")));

            imp.max_power_cap.set_info_label(&gpu.get_power_cap_max().map(|x| format!("{:.2} W", x)).unwrap_or_else(|_| gettextrs::gettext("N/A")));

            glib::Continue(true)
        });
        glib::timeout_add_seconds_local(1, gpu_usage_update);
    }
}
