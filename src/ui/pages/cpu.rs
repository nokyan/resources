use adw::{prelude::*, subclass::prelude::*};
use anyhow::Context;
use gtk::builders::FlowBoxChildBuilder;
use gtk::glib::{self, clone, timeout_future_seconds, MainContext, StrV};
use gtk::FlowBoxChild;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::ui::widgets::graph_box::ResGraphBox;
use crate::utils::units::{to_largest_unit, Base};
use crate::utils::{cpu, NaNDefault};

mod imp {
    use std::cell::RefCell;

    use crate::ui::widgets::{graph_box::ResGraphBox, info_box::ResInfoBox};

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/pages/cpu.ui")]
    pub struct ResCPU {
        #[template_child]
        pub cpu_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub logical_switch: TemplateChild<gtk::Switch>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub total_page: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub logical_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub total_cpu: TemplateChild<ResGraphBox>,
        #[template_child]
        pub thread_box: TemplateChild<gtk::FlowBox>,
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
        pub thread_graphs: RefCell<Vec<ResGraphBox>>,
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
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.instance();

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
        glib::Object::new::<Self>()
    }

    pub fn init(&self) {
        self.setup_widgets();
        self.setup_signals();
        self.setup_listener();
    }

    pub fn setup_widgets(&self) {
        let cpu_info = cpu::cpu_info()
            .with_context(|| "unable to get CPUInfo")
            .unwrap_or_default();
        let imp = self.imp();
        imp.cpu_name
            .set_label(&cpu_info.model_name.unwrap_or_else(|| i18n("N/A")));

        imp.total_cpu.set_title_label(&i18n("CPU"));
        imp.total_cpu.set_info_label(&i18n("N/A"));
        imp.total_cpu.set_data_points_max_amount(60);
        imp.total_cpu.set_graph_color(28, 113, 216);

        let logical_cpus = cpu_info.logical_cpus.unwrap_or(1);
        // if our CPU happens to only have one thread, showing a single thread box with the exact
        // same fraction as the progress bar for total CPU usage would be silly, so only do
        // thread boxes if we have more than one thread
        if logical_cpus > 1 {
            imp.logical_switch.set_sensitive(true);
            for i in 0..logical_cpus {
                let thread_box = ResGraphBox::new();
                thread_box.set_info_label(&i18n_f("CPU {}", &[&(i + 1).to_string()]));
                thread_box.set_title_label(&i18n("N/A"));
                thread_box.set_graph_height_request(72);
                thread_box.set_data_points_max_amount(60);
                thread_box.set_graph_color(28, 113, 216);
                let flow_box_chld = FlowBoxChild::builder()
                    .child(&thread_box)
                    .css_classes(vec!["tile", "card"])
                    .build();
                imp.thread_box.append(&flow_box_chld);
                imp.thread_graphs.borrow_mut().push(thread_box);
            }
        }

        imp.max_speed
            .set_info_label(&cpu_info.max_speed.map_or_else(
                || i18n("N/A"),
                |x| {
                    format!(
                        "{:.2} {}Hz",
                        to_largest_unit(x.into(), &Base::Decimal).0,
                        to_largest_unit(x.into(), &Base::Decimal).1
                    )
                },
            ));

        imp.logical_cpus.set_info_label(
            &cpu_info
                .logical_cpus
                .map_or_else(|| i18n("N/A"), |x| x.to_string()),
        );

        imp.physical_cpus.set_info_label(
            &cpu_info
                .physical_cpus
                .map_or_else(|| i18n("N/A"), |x| x.to_string()),
        );

        imp.sockets.set_info_label(
            &cpu_info
                .sockets
                .map_or_else(|| i18n("N/A"), |x| x.to_string()),
        );

        imp.virtualization
            .set_info_label(&cpu_info.virtualization.unwrap_or_else(|| i18n("N/A")));

        imp.architecture
            .set_info_label(&cpu_info.architecture.unwrap_or_else(|| i18n("N/A")));
    }

    pub fn setup_signals(&self) {
        let imp = self.imp();
        imp.logical_switch
            .connect_active_notify(clone!(@weak self as this => move |switch| {
                let imp = this.imp();
                if switch.is_active() {
                    imp.stack.set_visible_child(&imp.logical_page.get());
                } else {
                    imp.stack.set_visible_child(&imp.total_page.get());
                }
            }));
    }

    pub fn setup_listener(&self) {
        let main_context = MainContext::default();
        let thread_usage_update = clone!(@strong self as this => async move {
            let imp = this.imp();
            let cpu_info = cpu::cpu_info()
                .with_context(|| "unable to get CPUInfo")
                .unwrap_or_default();
            let logical_cpus = cpu_info.logical_cpus.unwrap_or(0);
            let mut old_total_usage = cpu::get_cpu_usage(None).await.unwrap_or((0, 0));
            let mut old_thread_usages: Vec<(u64, u64)> = Vec::new();
            loop {
                for i in 0..logical_cpus {
                    old_thread_usages.push(cpu::get_cpu_usage(Some(i)).await.unwrap_or((0,0)));
                }

                let new_total_usage = cpu::get_cpu_usage(None).await.unwrap_or((0,0));
                let idle_total_delta = new_total_usage.0 - old_total_usage.0;
                let sum_total_delta = new_total_usage.1 - old_total_usage.1;
                let work_total_time = sum_total_delta - idle_total_delta;
                let total_fraction = ((work_total_time as f64) / (sum_total_delta as f64)).nan_default(0.0);
                imp.total_cpu.push_data_point(total_fraction);
                imp.total_cpu.set_info_label(&format!("{} %", (total_fraction*100.0).round()));
                old_total_usage = new_total_usage;

                if logical_cpus > 1 {
                    for (i, old_thread_usage) in old_thread_usages.iter_mut().enumerate().take(logical_cpus) {
                        let new_thread_usage = cpu::get_cpu_usage(Some(i)).await.unwrap_or((0,0));
                        let idle_thread_delta = new_thread_usage.0 - old_thread_usage.0;
                        let sum_thread_delta = new_thread_usage.1 - old_thread_usage.1;
                        let work_thread_time = sum_thread_delta - idle_thread_delta;
                        let curr_threadbox = &imp.thread_graphs.borrow()[i];
                        let thread_fraction = ((work_thread_time as f64) / (sum_thread_delta as f64)).nan_default(0.0);
                        curr_threadbox.push_data_point(thread_fraction);
                        curr_threadbox.set_title_label(&format!("{} %", (thread_fraction*100.0).round()));
                        if let Ok(freq) = cpu::get_cpu_freq(i) {
                            let (frequency, base) = to_largest_unit(freq as f64, &Base::Decimal);
                            curr_threadbox.set_info_label(&format!("{frequency:.2} {base}Hz"));
                        }
                        *old_thread_usage = new_thread_usage;
                    }
                }

                timeout_future_seconds(1).await;
            }
        });

        main_context.spawn_local(thread_usage_update);
    }
}
