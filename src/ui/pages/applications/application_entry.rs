use gtk::{
    glib::{self},
    subclass::prelude::ObjectSubclassIsExt,
};

use crate::utils::app::AppItem;

mod imp {
    use std::cell::{Cell, RefCell};

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
        icon: RefCell<Icon>,

        #[property(get, set)]
        cpu_usage: Cell<f32>,
        #[property(get, set)]
        memory_usage: Cell<u64>,

        pub app_item: RefCell<Option<AppItem>>,
    }

    impl Default for ApplicationEntry {
        fn default() -> Self {
            Self {
                name: Cell::new(glib::GString::default()),
                id: Cell::new(None),
                description: Cell::new(None),
                icon: RefCell::new(ThemedIcon::new("generic-process").into()),

                cpu_usage: Cell::new(0.0),
                memory_usage: Cell::new(0),

                app_item: RefCell::new(None),
            }
        }
    }

    impl ApplicationEntry {
        pub fn name(&self) -> glib::GString {
            let name = self.name.take();
            let result = name.clone();
            self.name.set(name);
            result
        }

        pub fn set_name(&self, name: &str) {
            self.name.set(glib::GString::from(name));
        }

        pub fn description(&self) -> Option<glib::GString> {
            let description = self.description.take();
            let result = description.clone();
            self.description.set(description);
            result
        }

        pub fn set_description(&self, description: Option<&str>) {
            self.description.set(description.map(glib::GString::from));
        }

        pub fn id(&self) -> Option<glib::GString> {
            let id = self.id.take();
            let result = id.clone();
            self.id.set(id);
            result
        }

        pub fn set_id(&self, id: Option<&str>) {
            self.id.set(id.map(glib::GString::from));
        }

        pub fn icon(&self) -> Icon {
            let icon = self
                .icon
                .replace_with(|_| ThemedIcon::new("generic-process").into());
            let result = icon.clone();
            self.icon.set(icon);
            result
        }

        pub fn set_icon(&self, icon: &Icon) {
            self.icon.set(icon.clone());
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
        this.set_cpu_usage(app_item.cpu_time_ratio);
        this.set_memory_usage(app_item.memory_usage as u64);
        this.imp().app_item.replace(Some(app_item));
        this
    }

    pub fn update(&self, app_item: AppItem) {
        self.set_cpu_usage(app_item.cpu_time_ratio);
        self.set_memory_usage(app_item.memory_usage as u64);
        self.imp().app_item.replace(Some(app_item));
    }

    pub fn app_item(&self) -> Option<AppItem> {
        let imp = self.imp();
        let item = imp.app_item.take();
        imp.app_item.replace(item.clone());
        item
    }
}
