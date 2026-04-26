use std::time::{Duration, SystemTime};

use adw::{glib::property::PropertySet, prelude::*, subclass::prelude::*};
use gtk::glib;
use log::trace;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::ui::{set_subtitle_boolean_maybe, set_subtitle_converted_maybe};
use crate::utils::drive::{Drive, DriveData};
use crate::utils::units::convert_storage;

pub const TAB_ID_PREFIX: &str = "drive";

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use crate::ui::{pages::DRIVE_PRIMARY_ORD, widgets::graph_box::ResGraphBox};

    use super::*;

    use gtk::{
        CompositeTemplate,
        gio::Icon,
        glib::{ParamSpec, Properties, Value},
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/drive.ui")]
    #[properties(wrapper_type = super::ResDrive)]
    pub struct ResDrive {
        #[template_child]
        pub total_usage: TemplateChild<ResGraphBox>,
        #[template_child]
        pub read_speed: TemplateChild<ResGraphBox>,
        #[template_child]
        pub write_speed: TemplateChild<ResGraphBox>,
        #[template_child]
        pub total_read: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub total_written: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_type: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub device: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub capacity: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub writable: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub removable: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub link: TemplateChild<adw::ActionRow>,
        pub old_stats: RefCell<HashMap<String, usize>>,
        pub last_timestamp: Cell<SystemTime>,

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get)]
        main_graph_color: glib::Bytes,

        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: RefCell<Icon>,

        #[property(get, set)]
        usage: Cell<f64>,

        #[property(get = Self::tab_name, set = Self::set_tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,

        #[property(get = Self::tab_detail_string, set = Self::set_tab_detail_string, type = glib::GString)]
        tab_detail_string: Cell<glib::GString>,

        #[property(get = Self::tab_usage_string, set = Self::set_tab_usage_string, type = glib::GString)]
        tab_usage_string: Cell<glib::GString>,

        #[property(get = Self::tab_id, set = Self::set_tab_id, type = glib::GString)]
        tab_id: Cell<glib::GString>,

        #[property(get)]
        graph_locked_max_y: Cell<bool>,

        #[property(get)]
        primary_ord: Cell<u32>,

        #[property(get, set)]
        secondary_ord: Cell<u32>,
    }

    impl ResDrive {
        gstring_getter_setter!(tab_name, tab_detail_string, tab_usage_string, tab_id);

        pub fn icon(&self) -> Icon {
            let icon = self.icon.replace_with(|_| Drive::default_icon());
            let result = icon.clone();
            self.icon.set(icon);
            result
        }

        pub fn set_icon(&self, icon: &Icon) {
            self.icon.set(icon.clone());
        }
    }

    impl Default for ResDrive {
        fn default() -> Self {
            Self {
                total_usage: Default::default(),
                read_speed: Default::default(),
                write_speed: Default::default(),
                drive_type: Default::default(),
                total_read: Default::default(),
                total_written: Default::default(),
                device: Default::default(),
                capacity: Default::default(),
                writable: Default::default(),
                removable: Default::default(),
                link: Default::default(),
                uses_progress_bar: Cell::new(true),
                main_graph_color: glib::Bytes::from_static(&super::ResDrive::MAIN_GRAPH_COLOR),
                icon: RefCell::new(Drive::default_icon()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("Drive"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::new()),
                old_stats: Default::default(),
                last_timestamp: Cell::new(
                    SystemTime::now()
                        .checked_sub(Duration::from_secs(1))
                        .unwrap(),
                ),
                tab_usage_string: Cell::new(glib::GString::new()),
                graph_locked_max_y: Cell::new(true),
                primary_ord: Cell::new(DRIVE_PRIMARY_ORD),
                secondary_ord: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResDrive {
        const NAME: &'static str = "ResDrive";
        type Type = super::ResDrive;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResDrive {
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

    impl WidgetImpl for ResDrive {}
    impl BinImpl for ResDrive {}
}

glib::wrapper! {
    pub struct ResDrive(ObjectSubclass<imp::ResDrive>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Accessible;
}

impl Default for ResDrive {
    fn default() -> Self {
        Self::new()
    }
}

impl ResDrive {
    const MAIN_GRAPH_COLOR: [u8; 3] = [0xff, 0x78, 0x00];
    const SECTOR_SIZE: usize = 512;

    pub fn new() -> Self {
        trace!("Creating ResDrive GObject…");

        glib::Object::new::<Self>()
    }

    pub fn init(&self, drive_data: &DriveData, secondary_ord: u32) {
        self.set_secondary_ord(secondary_ord);
        self.setup_widgets(drive_data);
    }

    pub fn setup_widgets(&self, drive_data: &DriveData) {
        trace!(
            "Setting up ResDrive ({}) widgets…",
            drive_data.inner.sysfs_path.to_string_lossy()
        );

        let imp = self.imp();
        let drive = &drive_data.inner;

        let tab_id = format!(
            "{}-{}",
            TAB_ID_PREFIX,
            drive
                .model
                .as_deref()
                .unwrap_or(drive.sysfs_path.to_str().unwrap())
        );
        imp.set_tab_id(&tab_id);

        imp.set_icon(&drive.icon());
        imp.set_tab_name(&drive.display_name());

        imp.total_usage.set_title_label(&i18n("Drive Activity"));
        imp.total_usage.graph().set_graph_color(
            Self::MAIN_GRAPH_COLOR[0],
            Self::MAIN_GRAPH_COLOR[1],
            Self::MAIN_GRAPH_COLOR[2],
        );

        imp.read_speed.set_title_label(&i18n("Read Speed"));
        imp.read_speed.graph().set_graph_color(0xe6, 0x61, 0x00);
        imp.read_speed.graph().set_locked_max_y(None);

        imp.write_speed.set_title_label(&i18n("Write Speed"));
        imp.write_speed.graph().set_graph_color(0xc6, 0x46, 0x00);
        imp.write_speed.graph().set_locked_max_y(None);

        imp.drive_type.set_subtitle(&drive.drive_type.to_string());

        imp.device.set_subtitle(&drive.block_device);

        imp.last_timestamp.set(
            SystemTime::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap(),
        );

        if let Some(model_name) = &drive_data.inner.model {
            imp.set_tab_detail_string(&format!(
                "{model_name} ({})",
                &drive_data.inner.block_device
            ));
        } else {
            imp.set_tab_detail_string(&drive_data.inner.block_device);
        }

        imp.old_stats
            .borrow_mut()
            .clone_from(&drive_data.disk_stats);
    }

    pub fn refresh_page(&self, drive_data: DriveData) {
        trace!(
            "Refreshing ResDrive ({})…",
            drive_data.inner.sysfs_path.to_string_lossy()
        );

        let imp = self.imp();

        let DriveData {
            inner,
            is_virtual: _,
            writable,
            removable,
            disk_stats,
            capacity,
            link,
        } = drive_data;

        let read_sectors = disk_stats.get("read_sectors");
        let write_sectors = disk_stats.get("write_sectors");

        let time_passed = SystemTime::now()
            .duration_since(imp.last_timestamp.get())
            .map_or(1.0f64, |timestamp| timestamp.as_secs_f64());

        self.set_property("tab_name", inner.display_name());

        let usage_fraction = if let (
            Some(read_ticks),
            Some(write_ticks),
            Some(old_read_ticks),
            Some(old_write_ticks),
        ) = (
            disk_stats.get("read_ticks"),
            disk_stats.get("write_ticks"),
            imp.old_stats.borrow().get("read_ticks"),
            imp.old_stats.borrow().get("write_ticks"),
        ) {
            let delta_read_ticks = read_ticks.saturating_sub(*old_read_ticks);
            let delta_write_ticks = write_ticks.saturating_sub(*old_write_ticks);
            let read_ratio = delta_read_ticks as f64 / (time_passed * 1000.0);
            let write_ratio = delta_write_ticks as f64 / (time_passed * 1000.0);

            Some(f64::max(read_ratio, write_ratio).clamp(0.0, 1.0))
        } else {
            None
        };

        imp.total_usage.add_fraction_point(usage_fraction);

        self.set_property("usage", usage_fraction.unwrap_or_default());

        let read_speed = if let (Some(read_sectors), Some(old_read_sectors)) =
            (read_sectors, imp.old_stats.borrow().get("read_sectors"))
        {
            let delta_read_sectors = read_sectors.saturating_sub(*old_read_sectors);

            Some((delta_read_sectors.saturating_mul(Self::SECTOR_SIZE)) as f64 / time_passed)
        } else {
            None
        };

        let read_speed_string = imp.read_speed.add_speed_point(read_speed);

        let write_speed = if let (Some(write_sectors), Some(old_write_sectors)) =
            (write_sectors, imp.old_stats.borrow().get("write_sectors"))
        {
            let delta_write_sectors = write_sectors.saturating_sub(*old_write_sectors);

            Some((delta_write_sectors.saturating_mul(Self::SECTOR_SIZE)) as f64 / time_passed)
        } else {
            None
        };

        let write_speed_string = imp.write_speed.add_speed_point(write_speed);

        set_subtitle_converted_maybe(
            read_sectors.map(|sectors| sectors.saturating_mul(Self::SECTOR_SIZE) as f64),
            |bytes| convert_storage(bytes, false),
            &imp.total_read,
        );

        set_subtitle_converted_maybe(
            write_sectors.map(|sectors| sectors.saturating_mul(Self::SECTOR_SIZE) as f64),
            |bytes| convert_storage(bytes, false),
            &imp.total_written,
        );

        set_subtitle_converted_maybe(
            capacity.ok(),
            |bytes| convert_storage(bytes as f64, false),
            &imp.capacity,
        );

        set_subtitle_boolean_maybe(writable.ok(), &imp.writable);

        set_subtitle_boolean_maybe(removable.ok(), &imp.removable);

        if let Ok(link) = link {
            imp.link.set_subtitle(&link.to_string());
        } else {
            imp.link.set_subtitle(&i18n("N/A"));
        }

        self.set_property(
            "tab_usage_string",
            // Translators: This is an abbreviation for "Read" and "Write". This is displayed in the sidebar so your
            // translation should preferably be quite short or an abbreviation
            i18n_f("R: {} · W: {}", &[&read_speed_string, &write_speed_string]),
        );

        *imp.old_stats.borrow_mut() = disk_stats;
        imp.last_timestamp.set(SystemTime::now());
    }
}
