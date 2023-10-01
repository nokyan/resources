use std::time::Duration;

use adw::{prelude::*, subclass::prelude::*};
use anyhow::Context;
use gtk::glib::{self, clone, timeout_future, MainContext};
use gtk::FlowBoxChild;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::ui::widgets::graph_box::ResGraphBox;
use crate::utils::settings::SETTINGS;
use crate::utils::units::{convert_frequency, convert_temperature};
use crate::utils::{cpu, NaNDefault};

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::widgets::graph_box::ResGraphBox;

    use super::*;

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/me/nalux/Resources/ui/pages/cpu.ui")]
    #[properties(wrapper_type = super::ResCPU)]
    pub struct ResCPU {
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
        pub max_speed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub logical_cpus: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub physical_cpus: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub sockets: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub virtualization: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub architecture: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub temperature: TemplateChild<adw::ActionRow>,
        pub thread_graphs: RefCell<Vec<ResGraphBox>>,

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get)]
        icon: RefCell<Icon>,

        #[property(get, set)]
        usage: Cell<f64>,

        #[property(get = Self::tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,
    }

    impl ResCPU {
        pub fn tab_name(&self) -> glib::GString {
            let tab_name = self.tab_name.take();
            let result = tab_name.clone();
            self.tab_name.set(tab_name);
            result
        }
    }

    impl Default for ResCPU {
        fn default() -> Self {
            Self {
                logical_switch: Default::default(),
                stack: Default::default(),
                total_page: Default::default(),
                logical_page: Default::default(),
                total_cpu: Default::default(),
                thread_box: Default::default(),
                max_speed: Default::default(),
                logical_cpus: Default::default(),
                physical_cpus: Default::default(),
                sockets: Default::default(),
                virtualization: Default::default(),
                architecture: Default::default(),
                temperature: Default::default(),
                thread_graphs: Default::default(),
                uses_progress_bar: Cell::new(true),
                icon: RefCell::new(ThemedIcon::new("processor-symbolic").into()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("Processor"))),
            }
        }
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

    pub fn init(&self, window_title: &adw::WindowTitle) {
        self.setup_widgets(window_title);
        self.setup_signals();
        self.setup_listener();
    }

    pub fn setup_widgets(&self, window_title: &adw::WindowTitle) {
        let cpu_info = cpu::cpu_info()
            .with_context(|| "unable to get CPUInfo")
            .unwrap_or_default();
        let imp = self.imp();

        if let Some(cpu_name) = cpu_info.model_name {
            window_title.set_title(&cpu_name);
            window_title.set_subtitle(&i18n("Processor"));
        }

        imp.total_cpu.set_title_label(&i18n("CPU"));
        imp.total_cpu.set_subtitle(&i18n("N/A"));
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
                thread_box.set_subtitle(&i18n_f("CPU {}", &[&(i + 1).to_string()]));
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

        imp.max_speed.set_subtitle(
            &cpu_info
                .max_speed
                .map_or_else(|| i18n("N/A"), |x| convert_frequency(x as f64)),
        );

        imp.logical_cpus.set_subtitle(
            &cpu_info
                .logical_cpus
                .map_or_else(|| i18n("N/A"), |x| x.to_string()),
        );

        imp.physical_cpus.set_subtitle(
            &cpu_info
                .physical_cpus
                .map_or_else(|| i18n("N/A"), |x| x.to_string()),
        );

        imp.sockets.set_subtitle(
            &cpu_info
                .sockets
                .map_or_else(|| i18n("N/A"), |x| x.to_string()),
        );

        imp.virtualization
            .set_subtitle(&cpu_info.virtualization.unwrap_or_else(|| i18n("N/A")));

        imp.architecture
            .set_subtitle(&cpu_info.architecture.unwrap_or_else(|| i18n("N/A")));
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
            let mut old_thread_usages: Vec<(u64, u64)> = Vec::with_capacity(logical_cpus);

            imp.max_speed.set_subtitle(
                &cpu_info
                    .max_speed
                    .map_or_else(|| i18n("N/A"), |x| convert_frequency(x as f64)),
            );

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
                imp.total_cpu.set_subtitle(&format!("{} %", (total_fraction*100.0).round()));
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
                        curr_threadbox.set_title_label(&format!("{} %", (thread_fraction*100.0).round()));
                        if let Ok(freq) = cpu::get_cpu_freq(i) {
                            curr_threadbox.set_subtitle(&convert_frequency(freq as f64));
                        }
                        *old_thread_usage = new_thread_usage;
                    }
                }

                let temperature = cpu::get_temperature().await;
                if let Ok(temp) = temperature {
                    imp.temperature.set_subtitle(&convert_temperature(temp as f64));
                } else {
                    imp.temperature.set_subtitle(&i18n("N/A"));
                }

                this.set_property("usage", total_fraction);

                timeout_future(Duration::from_secs_f32(SETTINGS.refresh_speed().ui_refresh_interval())).await;
            }
        });

        main_context.spawn_local(thread_usage_update);
    }
}
