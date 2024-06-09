use std::time::{Duration, SystemTime};

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::utils::drive::{Drive, DriveData};
use crate::utils::units::{convert_speed, convert_storage};

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use crate::ui::widgets::graph_box::ResGraphBox;

    use super::*;

    use gtk::{
        gio::Icon,
        glib::{ParamSpec, Properties, Value},
        CompositeTemplate,
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

        #[property(get = Self::tab_detail, set = Self::set_tab_detail, type = glib::GString)]
        tab_detail_string: Cell<glib::GString>,

        #[property(get = Self::tab_usage_string, set = Self::set_tab_usage_string, type = glib::GString)]
        tab_usage_string: Cell<glib::GString>,

        #[property(get = Self::tab_id, set = Self::set_tab_id, type = glib::GString)]
        tab_id: Cell<glib::GString>,

        #[property(get)]
        graph_locked_max_y: Cell<bool>,
    }

    impl ResDrive {
        pub fn tab_name(&self) -> glib::GString {
            let tab_name = self.tab_name.take();
            let result = tab_name.clone();
            self.tab_name.set(tab_name);
            result
        }

        pub fn set_tab_name(&self, tab_name: &str) {
            self.tab_name.set(glib::GString::from(tab_name));
        }

        pub fn tab_detail(&self) -> glib::GString {
            let detail = self.tab_detail_string.take();
            let result = detail.clone();
            self.tab_detail_string.set(detail);
            result
        }

        pub fn set_tab_detail(&self, detail: &str) {
            self.tab_detail_string.set(glib::GString::from(detail));
        }

        pub fn icon(&self) -> Icon {
            let icon = self.icon.replace_with(|_| Drive::default_icon());
            let result = icon.clone();
            self.icon.set(icon);
            result
        }

        pub fn set_icon(&self, icon: &Icon) {
            self.icon.set(icon.clone());
        }

        pub fn tab_usage_string(&self) -> glib::GString {
            let tab_usage_string = self.tab_usage_string.take();
            let result = tab_usage_string.clone();
            self.tab_usage_string.set(tab_usage_string);
            result
        }

        pub fn set_tab_usage_string(&self, tab_usage_string: &str) {
            self.tab_usage_string
                .set(glib::GString::from(tab_usage_string));
        }

        pub fn tab_id(&self) -> glib::GString {
            let tab_id = self.tab_id.take();
            let result = tab_id.clone();
            self.tab_id.set(tab_id);
            result
        }

        pub fn set_tab_id(&self, tab_id: &str) {
            self.tab_id.set(glib::GString::from(tab_id));
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
        @extends gtk::Widget, adw::Bin;
}

impl ResDrive {
    const ID_PREFIX: &'static str = "drive";
    const MAIN_GRAPH_COLOR: [u8; 3] = [246, 211, 45];
    const SECTOR_SIZE: usize = 512;

    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, drive_data: &DriveData) {
        self.setup_widgets(drive_data);
    }

    pub fn setup_widgets(&self, drive_data: &DriveData) {
        let imp = self.imp();
        let drive = &drive_data.inner;

        let tab_id = format!(
            "{}-{}",
            Self::ID_PREFIX,
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
        imp.read_speed.graph().set_graph_color(229, 165, 10);
        imp.read_speed.graph().set_locked_max_y(None);

        imp.write_speed.set_title_label(&i18n("Write Speed"));
        imp.write_speed.graph().set_graph_color(229, 165, 10);
        imp.write_speed.graph().set_locked_max_y(None);

        imp.drive_type.set_subtitle(&drive.drive_type.to_string());

        imp.device.set_subtitle(&drive.block_device);

        imp.last_timestamp.set(
            SystemTime::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap(),
        );

        if let Some(model_name) = &drive_data.inner.model {
            imp.set_tab_detail(model_name);
        } else {
            imp.set_tab_detail(&drive_data.inner.block_device);
        }

        *imp.old_stats.borrow_mut() = drive_data.disk_stats.clone();
    }

    pub fn refresh_page(&self, drive_data: DriveData) {
        let imp = self.imp();

        let DriveData {
            inner: _,
            is_virtual: _,
            writable,
            removable,
            disk_stats,
            capacity,
        } = drive_data;

        let time_passed = SystemTime::now()
            .duration_since(imp.last_timestamp.get())
            .map_or(1.0f64, |timestamp| timestamp.as_secs_f64());

        if let (Some(read_ticks), Some(write_ticks), Some(old_read_ticks), Some(old_write_ticks)) = (
            disk_stats.get("read_ticks"),
            disk_stats.get("write_ticks"),
            imp.old_stats.borrow().get("read_ticks"),
            imp.old_stats.borrow().get("write_ticks"),
        ) {
            let delta_read_ticks = read_ticks.saturating_sub(*old_read_ticks);
            let delta_write_ticks = write_ticks.saturating_sub(*old_write_ticks);
            let read_ratio = delta_read_ticks as f64 / (time_passed * 1000.0);
            let write_ratio = delta_write_ticks as f64 / (time_passed * 1000.0);

            let total_usage = f64::max(read_ratio, write_ratio).clamp(0.0, 1.0);

            let percentage_string = format!("{} %", (total_usage * 100.0).round());

            imp.total_usage.graph().set_visible(true);
            imp.total_usage.graph().push_data_point(total_usage);
            imp.total_usage.set_subtitle(&percentage_string);

            self.set_property("usage", total_usage);
        } else {
            imp.total_usage.graph().set_visible(false);
            imp.total_usage.set_subtitle(&i18n("N/A"));

            self.set_property("usage", 0.0);
        }

        let read_string = if let (Some(read_sectors), Some(old_read_sectors)) = (
            disk_stats.get("read_sectors"),
            imp.old_stats.borrow().get("read_sectors"),
        ) {
            let delta_read_sectors = read_sectors.saturating_sub(*old_read_sectors);

            let read_speed = (delta_read_sectors * Self::SECTOR_SIZE) as f64 / time_passed;

            imp.read_speed.graph().set_visible(true);
            imp.read_speed.graph().push_data_point(read_speed);

            let highest_read_speed = imp.read_speed.graph().get_highest_value();

            let formatted_read_speed = convert_speed(read_speed, false);

            let formatted_highest_read_speed = convert_speed(highest_read_speed, false);

            imp.read_speed.set_subtitle(&format!(
                "{formatted_read_speed} · {} {formatted_highest_read_speed}",
                i18n("Highest:")
            ));

            formatted_read_speed
        } else {
            imp.read_speed.graph().set_visible(false);
            imp.read_speed.set_subtitle(&i18n("N/A"));

            i18n("N/A")
        };

        let write_string = if let (Some(write_sectors), Some(old_write_sectors)) = (
            disk_stats.get("write_sectors"),
            imp.old_stats.borrow().get("write_sectors"),
        ) {
            let delta_write_sectors = write_sectors.saturating_sub(*old_write_sectors);

            let write_speed = (delta_write_sectors * Self::SECTOR_SIZE) as f64 / time_passed;

            imp.read_speed.graph().set_visible(true);
            imp.write_speed.graph().push_data_point(write_speed);

            let highest_write_speed = imp.write_speed.graph().get_highest_value();

            let formatted_write_speed = convert_speed(write_speed, false);

            let formatted_highest_write_speed = convert_speed(highest_write_speed, false);

            imp.write_speed.set_subtitle(&format!(
                "{formatted_write_speed} · {} {formatted_highest_write_speed}",
                i18n("Highest:")
            ));

            formatted_write_speed
        } else {
            imp.write_speed.graph().set_visible(false);
            imp.write_speed.set_subtitle(&i18n("N/A"));

            i18n("N/A")
        };

        if let (Some(read_sectors), Some(write_sectors)) = (
            disk_stats.get("read_sectors"),
            disk_stats.get("write_sectors"),
        ) {
            imp.total_read.set_subtitle(&convert_storage(
                (read_sectors * Self::SECTOR_SIZE) as f64,
                false,
            ));
            imp.total_written.set_subtitle(&convert_storage(
                (write_sectors * Self::SECTOR_SIZE) as f64,
                false,
            ));
        } else {
            imp.total_read.set_subtitle(&i18n("N/A"));
            imp.total_written.set_subtitle(&i18n("N/A"));
        }

        if let Ok(capacity) = capacity {
            imp.capacity
                .set_subtitle(&convert_storage(capacity as f64, false));
        } else {
            imp.capacity.set_subtitle(&i18n("N/A"));
        }

        if let Ok(writable) = writable {
            if writable {
                imp.writable.set_subtitle(&i18n("Yes"));
            } else {
                imp.writable.set_subtitle(&i18n("No"));
            }
        } else {
            imp.writable.set_subtitle(&i18n("N/A"));
        }

        if let Ok(removable) = removable {
            if removable {
                imp.removable.set_subtitle(&i18n("Yes"));
            } else {
                imp.removable.set_subtitle(&i18n("No"));
            }
        } else {
            imp.removable.set_subtitle(&i18n("N/A"));
        }

        self.set_property(
            "tab_usage_string",
            // Translators: This is an abbreviation for "Read" and "Write". This is displayed in the sidebar so your
            // translation should preferably be quite short or an abbreviation
            i18n_f("R: {} · W: {}", &[&read_string, &write_string]),
        );

        *imp.old_stats.borrow_mut() = disk_stats;
        imp.last_timestamp.set(SystemTime::now());
    }
}
