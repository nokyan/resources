use gtk::{
    glib::{self, GString},
    subclass::prelude::ObjectSubclassIsExt,
};
use process_data::Containerization;

use crate::{
    i18n::i18n,
    utils::{process::Process, TICK_RATE},
};

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        prelude::{Cast, ObjectExt},
        subclass::prelude::{DerivedObjectProperties, ObjectImpl, ObjectImplExt, ObjectSubclass},
    };

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::ProcessEntry)]
    pub struct ProcessEntry {
        #[property(get = Self::name, set = Self::set_name, type = glib::GString)]
        name: Cell<glib::GString>,

        #[property(get = Self::commandline, set = Self::set_commandline, type = glib::GString)]
        commandline: Cell<glib::GString>,

        #[property(get = Self::user, set = Self::set_user, type = glib::GString)]
        user: Cell<glib::GString>,

        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: Cell<Icon>,

        #[property(get, set)]
        pid: Cell<i32>,

        #[property(get, set)]
        cpu_usage: Cell<f32>,

        #[property(get, set)]
        memory_usage: Cell<u64>,

        #[property(get, set)]
        read_speed: Cell<f64>, // will be -1.0 if read data is not available

        #[property(get, set)]
        read_total: Cell<i64>, // will be -1 if read data is not available

        #[property(get, set)]
        write_speed: Cell<f64>, // will be -1.0 if write data is not available

        #[property(get, set)]
        write_total: Cell<i64>, // will be -1 if write data is not available

        #[property(get, set)]
        gpu_usage: Cell<f32>,

        #[property(get, set)]
        enc_usage: Cell<f32>,

        #[property(get, set)]
        dec_usage: Cell<f32>,

        #[property(get, set)]
        gpu_mem_usage: Cell<u64>,

        #[property(get, set)]
        total_cpu_time: Cell<f64>,

        #[property(get, set)]
        user_cpu_time: Cell<f64>,

        #[property(get, set)]
        system_cpu_time: Cell<f64>,

        #[property(get, set)]
        niceness: Cell<i8>,

        #[property(get = Self::cgroup, set = Self::set_cgroup)]
        cgroup: Cell<Option<glib::GString>>,

        #[property(get = Self::containerization, set = Self::set_containerization)]
        containerization: Cell<glib::GString>,

        #[property(get = Self::running_since, set = Self::set_running_since)]
        running_since: Cell<Option<glib::GString>>,

        // TODO: Make this properly dynamic, don't use a variable that's never read
        #[property(get = Self::symbolic)]
        #[allow(dead_code)]
        symbolic: Cell<bool>,

        pub affinity: RefCell<Vec<bool>>,
    }

    impl Default for ProcessEntry {
        fn default() -> Self {
            Self {
                name: Cell::new(glib::GString::default()),
                commandline: Cell::new(glib::GString::default()),
                user: Cell::new(glib::GString::default()),
                icon: Cell::new(ThemedIcon::new("generic-process").into()),
                pid: Cell::new(0),
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
                total_cpu_time: Cell::new(0.0),
                user_cpu_time: Cell::new(0.0),
                system_cpu_time: Cell::new(0.0),
                niceness: Cell::new(0),
                cgroup: Cell::new(None),
                containerization: Cell::new(glib::GString::default()),
                running_since: Cell::new(None),
                symbolic: Cell::new(false),
                affinity: Default::default(),
            }
        }
    }

    impl ProcessEntry {
        gstring_getter_setter!(user, commandline, name, containerization);
        gstring_option_getter_setter!(cgroup, running_since);

        pub fn icon(&self) -> Icon {
            let icon = self.icon.replace(ThemedIcon::new("generic-process").into());
            self.icon.set(icon.clone());
            icon
        }

        pub fn set_icon(&self, icon: &Icon) {
            self.icon.set(icon.clone());
        }

        pub fn symbolic(&self) -> bool {
            let icon = self.icon.replace(ThemedIcon::new("generic-process").into());
            self.icon.set(icon.clone());

            icon.downcast_ref::<ThemedIcon>()
                .is_some_and(|themed_icon| {
                    themed_icon
                        .names()
                        .iter()
                        .all(|name| name.ends_with("-symbolic"))
                        || themed_icon
                            .names()
                            .iter()
                            .all(|name| name.contains("system-processes"))
                        || themed_icon
                            .names()
                            .iter()
                            .all(|name| name.contains("generic-process"))
                })
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProcessEntry {
        const NAME: &'static str = "ProcessEntry";
        type Type = super::ProcessEntry;
    }

    impl ObjectImpl for ProcessEntry {
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
    pub struct ProcessEntry(ObjectSubclass<imp::ProcessEntry>);
}

impl ProcessEntry {
    pub fn new(process: &Process) -> Self {
        let containerization = match process.data.containerization {
            Containerization::None => i18n("No"),
            Containerization::Flatpak => i18n("Yes (Flatpak)"),
            Containerization::Snap => i18n("Yes (Snap)"),
        };

        let this: Self = glib::Object::builder()
            .property("name", &process.display_name)
            .property("commandline", process.data.commandline.replace('\0', " "))
            .property("user", &process.data.user)
            .property("icon", &process.icon)
            .property("pid", process.data.pid)
            .property("cgroup", process.data.cgroup.clone().map(GString::from))
            .property("containerization", containerization)
            .property("running_since", process.running_since().ok())
            .build();
        this.update(process);
        this
    }

    pub fn update(&self, process: &Process) {
        self.set_cpu_usage(process.cpu_time_ratio());
        self.set_memory_usage(process.data.memory_usage as u64);
        self.set_read_speed(process.read_speed().unwrap_or(-1.0));
        self.set_read_total(
            process
                .data
                .read_bytes
                .map_or(-1, |read_total| read_total as i64),
        );
        self.set_write_speed(process.write_speed().unwrap_or(-1.0));
        self.set_write_total(
            process
                .data
                .write_bytes
                .map_or(-1, |write_total| write_total as i64),
        );
        self.set_gpu_usage(process.gpu_usage());
        self.set_enc_usage(process.enc_usage());
        self.set_dec_usage(process.dec_usage());
        self.set_gpu_mem_usage(process.gpu_mem_usage());
        self.set_user_cpu_time((process.data.user_cpu_time as f64) / (*TICK_RATE as f64));
        self.set_system_cpu_time((process.data.system_cpu_time as f64) / (*TICK_RATE as f64));
        self.set_total_cpu_time(self.user_cpu_time() + self.system_cpu_time());
        self.set_niceness(*process.data.niceness);
        *self.imp().affinity.borrow_mut() = process.data.affinity.clone();
    }

    pub fn affinity(&self) -> Vec<bool> {
        self.imp().affinity.borrow().clone()
    }
}
