use gtk::{
    glib::{self},
    subclass::prelude::ObjectSubclassIsExt,
};

use crate::utils::app::AppItem;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::Cast;
    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        prelude::ObjectExt,
        subclass::prelude::{DerivedObjectProperties, ObjectImpl, ObjectImplExt, ObjectSubclass},
    };

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

        // TODO: Make this properly dynamic, don't use a variable that's never read
        #[property(get = Self::symbolic)]
        #[allow(dead_code)]
        symbolic: Cell<bool>,

        pub app_item: RefCell<Option<AppItem>>,
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
                app_item: RefCell::new(None),
                gpu_usage: Cell::new(0.0),
                enc_usage: Cell::new(0.0),
                dec_usage: Cell::new(0.0),
                gpu_mem_usage: Cell::new(0),
                symbolic: Cell::new(false),
            }
        }
    }

    impl ApplicationEntry {
        pub fn name(&self) -> glib::GString {
            let name = self.name.take();
            self.name.set(name.clone());
            name
        }

        pub fn set_name(&self, name: &str) {
            self.name.set(glib::GString::from(name));
        }

        pub fn description(&self) -> Option<glib::GString> {
            let description = self.description.take();
            self.description.set(description.clone());
            description
        }

        pub fn set_description(&self, description: Option<&str>) {
            self.description.set(description.map(glib::GString::from));
        }

        pub fn id(&self) -> Option<glib::GString> {
            let id = self.id.take();
            self.id.set(id.clone());
            id
        }

        pub fn set_id(&self, id: Option<&str>) {
            self.id.set(id.map(glib::GString::from));
        }

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
    pub fn new(app_item: AppItem) -> Self {
        let this: Self = glib::Object::builder()
            .property("name", &app_item.display_name)
            .property("icon", &app_item.icon)
            .property("id", &app_item.id)
            .build();
        this.update(app_item);
        this
    }

    pub fn update(&self, app_item: AppItem) {
        self.set_cpu_usage(app_item.cpu_time_ratio);
        self.set_memory_usage(app_item.memory_usage as u64);
        self.set_read_speed(app_item.read_speed);
        self.set_read_total(app_item.read_total);
        self.set_write_speed(app_item.write_speed);
        self.set_write_total(app_item.write_total);
        self.set_gpu_usage(app_item.gpu_usage);
        self.set_enc_usage(app_item.enc_usage);
        self.set_dec_usage(app_item.dec_usage);
        self.set_gpu_mem_usage(app_item.gpu_mem_usage);

        self.imp().app_item.replace(Some(app_item));
    }

    pub fn app_item(&self) -> Option<AppItem> {
        let imp = self.imp();
        let item = imp.app_item.take();
        imp.app_item.replace(item.clone());
        item
    }
}
