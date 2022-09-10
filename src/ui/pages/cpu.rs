use adw::{prelude::*, subclass::prelude::*};
use anyhow::Context;
use gtk::glib::{self, clone};
use gtk::subclass::prelude::*;

use crate::config::PROFILE;
use crate::ui::widgets::progress_box::ResProgressBox;
use crate::utils::units::{to_largest_unit, Base};
use crate::utils::{cpu, NaNDefault};

mod imp {
    use std::cell::RefCell;

    use crate::ui::widgets::info_box::ResInfoBox;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/pages/cpu.ui")]
    pub struct ResCPU {
        pub thread_boxes: RefCell<Vec<ResProgressBox>>,
        #[template_child]
        pub cpu_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub threads_expander: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub total_usage_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub max_speed: TemplateChild<ResInfoBox>,
        #[template_child]
        pub logical_cpus: TemplateChild<ResInfoBox>,
        #[template_child]
        pub physical_cpus: TemplateChild<ResInfoBox>,
        #[template_child]
        pub sockets: TemplateChild<ResInfoBox>,
        #[template_child]
        pub virtualization: TemplateChild<ResInfoBox>,
        #[template_child]
        pub architecture: TemplateChild<ResInfoBox>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResCPU {
        const NAME: &'static str = "ResCPU";
        type Type = super::ResCPU;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResCPU {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResCPU {}
    impl BinImpl for ResCPU {}
}

glib::wrapper! {
    pub struct ResCPU(ObjectSubclass<imp::ResCPU>)
        @extends gtk::Widget, adw::Bin;
}

impl ResCPU {
    pub fn new() -> Self {
        let page = glib::Object::new::<Self>(&[]).expect("Failed to create ResCPU");
        page.init();
        page
    }

    pub fn init(&self) {
        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        let cpu_info = cpu::cpu_info()
            .with_context(|| "unable to get CPUInfo")
            .unwrap();
        let imp = self.imp();
        imp.cpu_name.set_label(
            &cpu_info
                .model_name
                .unwrap_or_else(|| gettextrs::gettext("N/A")),
        );

        let logical_cpus = cpu_info.logical_cpus.unwrap_or(1);
        // if our CPU happens to only have one thread, showing a single thread box with the exact
        // same fraction as the progress bar for total CPU usage would be silly, so only do
        // thread boxes if we have more than one thread
        if logical_cpus > 1 {
            for i in 0..logical_cpus {
                let progress_box = ResProgressBox::new();
                progress_box.set_title(&format!("CPU {}", i + 1));
                progress_box.set_percentage_label("0 %");
                imp.threads_expander.add_row(&progress_box);
                imp.thread_boxes.borrow_mut().push(progress_box);
            }
        }
        imp.max_speed.set_info_label(
            &cpu_info
                .max_speed
                .map(|x| {
                    format!(
                        "{:.2} {}Hz",
                        to_largest_unit(x.into(), Base::Decimal).0,
                        to_largest_unit(x.into(), Base::Decimal).1
                    )
                })
                .unwrap_or_else(|| gettextrs::gettext("N/A")),
        );
        imp.logical_cpus.set_info_label(
            &cpu_info
                .logical_cpus
                .map(|x| x.to_string())
                .unwrap_or_else(|| gettextrs::gettext("N/A")),
        );
        imp.physical_cpus.set_info_label(
            &cpu_info
                .physical_cpus
                .map(|x| x.to_string())
                .unwrap_or_else(|| gettextrs::gettext("N/A")),
        );
        imp.sockets.set_info_label(
            &cpu_info
                .sockets
                .map(|x| x.to_string())
                .unwrap_or_else(|| gettextrs::gettext("N/A")),
        );
        imp.virtualization.set_info_label(
            &cpu_info
                .virtualization
                .unwrap_or_else(|| gettextrs::gettext("N/A")),
        );
        imp.architecture.set_info_label(
            &cpu_info
                .architecture
                .unwrap_or_else(|| gettextrs::gettext("N/A")),
        );
    }

    pub fn setup_signals(&self) {
        let cpu_info = cpu::cpu_info()
            .with_context(|| "unable to get CPUInfo")
            .unwrap();
        let logical_cpus = cpu_info.logical_cpus.unwrap_or(0);
        let mut old_total_usage = cpu::get_cpu_usage(None).unwrap_or((0, 0));
        let mut old_thread_usages: Vec<(u64, u64)> = Vec::new();

        let thread_usage_update = clone!(@strong self as this => move || {
            let imp = this.imp();
            for i in 0..logical_cpus {
                old_thread_usages.push(cpu::get_cpu_usage(Some(i)).unwrap_or((0,0)));
            }

            let new_total_usage = cpu::get_cpu_usage(None).unwrap_or((0,0));
            let idle_total_delta = new_total_usage.0 - old_total_usage.0;
            let sum_total_delta = new_total_usage.1 - old_total_usage.1;
            let work_total_time = sum_total_delta - idle_total_delta;
            let total_fraction = ((work_total_time as f64) / (sum_total_delta as f64)).nan_default(0.0);
            imp.total_usage_label.set_label(&format!("{} %", (total_fraction*100.0).round()));
            old_total_usage = new_total_usage;

            if logical_cpus > 1 {
                for (i, old_thread_usage) in old_thread_usages.iter_mut().enumerate().take(logical_cpus) {
                    let new_thread_usage = cpu::get_cpu_usage(Some(i)).unwrap_or((0,0));
                    let idle_thread_delta = new_thread_usage.0 - old_thread_usage.0;
                    let sum_thread_delta = new_thread_usage.1 - old_thread_usage.1;
                    let work_thread_time = sum_thread_delta - idle_thread_delta;
                    let curr_threadbox = &imp.thread_boxes.borrow()[i];
                    let thread_fraction = ((work_thread_time as f64) / (sum_thread_delta as f64)).nan_default(0.0);
                    curr_threadbox.set_fraction(thread_fraction);
                    curr_threadbox.set_percentage_label(&format!("{} %", (thread_fraction*100.0).round()));
                    if let Ok(freq) = cpu::get_cpu_freq(i) {
                        let (frequency, base) = to_largest_unit(freq as f64, Base::Decimal);
                        curr_threadbox.set_title(&format!("CPU {} · {:.2} {}Hz", i + 1, frequency, base));
                    }
                    *old_thread_usage = new_thread_usage;
                }
            }

            glib::Continue(true)
        });

        glib::timeout_add_seconds_local(1, thread_usage_update);
    }
}
