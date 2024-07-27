use gtk::glib::{self};
use process_data::Containerization;

use crate::{
    i18n::i18n,
    utils::app::{App, AppsContext},
};

mod imp {
    use std::cell::Cell;

    use glib::object::Cast;
    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        prelude::ObjectExt,
        subclass::prelude::{DerivedObjectProperties, ObjectImpl, ObjectImplExt, ObjectSubclass},
    };

    use crate::gstring_getter_setter;

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::ApplicationEntry)]
    pub struct ApplicationEntry {
        #[property(get = Self::name, set = Self::set_name, type = glib::GString)]
        name: Cell<glib::GString>,

        #[property(get = Self::id, set = Self::set_id, type = Option<glib::GString>)]
        id: Cell<Option<glib::GString>>,

        #[property(get = Self::description, set = Self::set_description, type = Option<glib::GString>)]
        description: Cell<Option<glib::GString>>,

        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: Cell<Icon>,

        #[property(get, set)]
        cpu_usage: Cell<f32>,

        #[property(get, set)]
        memory_usage: Cell<u64>,

        #[property(get, set)]
        read_speed: Cell<f64>,

        #[property(get, set)]
        read_total: Cell<u64>,

        #[property(get, set)]
        write_speed: Cell<f64>,

        #[property(get, set)]
        write_total: Cell<u64>,

        #[property(get, set)]
        gpu_usage: Cell<f32>,

        #[property(get, set)]
        enc_usage: Cell<f32>,

        #[property(get, set)]
        dec_usage: Cell<f32>,

        #[property(get, set)]
        gpu_mem_usage: Cell<u64>,

        #[property(get = Self::running_since, set = Self::set_running_since)]
        running_since: Cell<glib::GString>,

        #[property(get = Self::containerization, set = Self::set_containerization)]
        containerization: Cell<glib::GString>,

        #[property(get, set)]
        running_processes: Cell<u32>,

        // TODO: Make this properly dynamic, don't use a variable that's never read
        #[property(get = Self::symbolic)]
        #[allow(dead_code)]
        symbolic: Cell<bool>,
    }

    impl Default for ApplicationEntry {
        fn default() -> Self {
            Self {
                name: Cell::new(glib::GString::default()),
                id: Cell::new(None),
                description: Cell::new(None),
                icon: Cell::new(ThemedIcon::new("generic-process").into()),
                cpu_usage: Cell::new(0.0),
                memory_usage: Cell::new(0),
                read_speed: Cell::new(0.0),
                read_total: Cell::new(0),
                write_speed: Cell::new(0.0),
                write_total: Cell::new(0),
                gpu_usage: Cell::new(0.0),
                enc_usage: Cell::new(0.0),
                dec_usage: Cell::new(0.0),
                gpu_mem_usage: Cell::new(0),
                symbolic: Cell::new(false),
                running_since: Cell::new(glib::GString::default()),
                containerization: Cell::new(glib::GString::default()),
                running_processes: Cell::new(0),
            }
        }
    }

    impl ApplicationEntry {
        gstring_getter_setter!(name, running_since, containerization);

        gstring_option_getter_setter!(description, id);

        pub fn icon(&self) -> Icon {
            let icon = self.icon.replace(ThemedIcon::new("generic-process").into());
            self.icon.set(icon.clone());
            icon
        }

        pub fn set_icon(&self, icon: &Icon) {
            self.icon.set(icon.clone());
        }

        pub fn symbolic(&self) -> bool {
            let id = self.id.take();
            self.id.set(id.clone());

            let icon = self.icon.replace(ThemedIcon::new("generic-process").into());
            self.icon.set(icon.clone());

            id.is_none() // this will be the case for System Processes
                || icon
                    .downcast_ref::<ThemedIcon>()
                    .is_some_and(|themed_icon| {
                        themed_icon
                            .names()
                            .iter()
                            .all(|name| name.ends_with("-symbolic"))
                            || themed_icon
                                .names()
                                .iter()
                                .all(|name| name.contains("generic-process"))
                    })
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ApplicationEntry {
        const NAME: &'static str = "ApplicationEntry";
        type Type = super::ApplicationEntry;
    }

    impl ObjectImpl for ApplicationEntry {
        fn constructed(&self) {
            self.parent_constructed();
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
}

glib::wrapper! {
    pub struct ApplicationEntry(ObjectSubclass<imp::ApplicationEntry>);
}

impl ApplicationEntry {
    pub fn new(app: &App, apps_context: &AppsContext) -> Self {
        let this: Self = glib::Object::builder()
            .property("name", &app.display_name)
            .property("icon", &app.icon)
            .property("id", &app.id)
            .build();
        this.update(app, apps_context);
        this
    }

    pub fn update(&self, app: &App, apps_context: &AppsContext) {
        self.set_cpu_usage(app.cpu_time_ratio(apps_context));
        self.set_memory_usage(app.memory_usage(apps_context) as u64);
        self.set_read_speed(app.read_speed(apps_context));
        self.set_read_total(app.read_total(apps_context));
        self.set_write_speed(app.write_speed(apps_context));
        self.set_write_total(app.write_total(apps_context));
        self.set_gpu_usage(app.gpu_usage(apps_context));
        self.set_enc_usage(app.enc_usage(apps_context));
        self.set_dec_usage(app.dec_usage(apps_context));
        self.set_gpu_mem_usage(app.gpu_mem_usage(apps_context));

        let containerized = match app.containerization {
            Containerization::None => i18n("No"),
            Containerization::Flatpak => i18n("Yes (Flatpak)"),
            Containerization::Snap => i18n("Yes (Snap)"),
        };

        self.set_containerization(containerized);
        self.set_running_processes(app.running_processes() as u32);
        self.set_running_since(app.running_since(apps_context));
    }
}
