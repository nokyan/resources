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
        pub read: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub write: TemplateChild<adw::ActionRow>,
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
        pub drive: RefCell<Drive>,
        pub last_timestamp: Cell<SystemTime>,

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: RefCell<Icon>,

        #[property(get, set)]
        usage: Cell<f64>,

        #[property(get = Self::tab_name, set = Self::set_tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,

        #[property(get = Self::tab_subtitle, set = Self::set_tab_subtitle, type = glib::GString)]
        tab_subtitle: Cell<glib::GString>,
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

        pub fn icon(&self) -> Icon {
            let icon = self.icon.replace_with(|_| Drive::default_icon());
            let result = icon.clone();
            self.icon.set(icon);
            result
        }

        pub fn set_icon(&self, icon: &Icon) {
            self.icon.set(icon.clone());
        }

        pub fn tab_subtitle(&self) -> glib::GString {
            let tab_subtitle = self.tab_subtitle.take();
            let result = tab_subtitle.clone();
            self.tab_subtitle.set(tab_subtitle);
            result
        }

        pub fn set_tab_subtitle(&self, tab_subtitle: &str) {
            self.tab_subtitle.set(glib::GString::from(tab_subtitle));
        }
    }

    impl Default for ResDrive {
        fn default() -> Self {
            Self {
                total_usage: Default::default(),
                read: Default::default(),
                write: Default::default(),
                drive_type: Default::default(),
                device: Default::default(),
                capacity: Default::default(),
                writable: Default::default(),
                removable: Default::default(),
                uses_progress_bar: Cell::new(true),
                icon: RefCell::new(Drive::default_icon()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("Drive"))),
                old_stats: Default::default(),
                drive: Default::default(),
                last_timestamp: Cell::new(
                    SystemTime::now()
                        .checked_sub(Duration::from_secs(1))
                        .unwrap(),
                ),
                tab_subtitle: Cell::new(glib::GString::from("")),
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
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, drive_data: &DriveData) {
        self.setup_widgets(drive_data);
    }

    pub fn setup_widgets(&self, drive_data: &DriveData) {
        let imp = self.imp();
        let drive = &drive_data.inner;

        imp.set_icon(&drive.icon());
        imp.set_tab_name(&drive.display_name(drive_data.capacity as f64));

        imp.total_usage.set_title_label(&i18n("Total Usage"));
        imp.total_usage.set_data_points_max_amount(60);
        imp.total_usage.set_graph_color(229, 165, 10);
        imp.drive_type.set_subtitle(
            &(match drive.drive_type {
                crate::utils::drive::DriveType::CdDvdBluray => i18n("CD/DVD/Blu-ray Drive"),
                crate::utils::drive::DriveType::Emmc => i18n("eMMC Storage"),
                crate::utils::drive::DriveType::Flash => i18n("Flash Storage"),
                crate::utils::drive::DriveType::Floppy => i18n("Floppy Drive"),
                crate::utils::drive::DriveType::Hdd => i18n("Hard Disk Drive"),
                crate::utils::drive::DriveType::LoopDevice => i18n("Loop Device"),
                crate::utils::drive::DriveType::MappedDevice => i18n("Mapped Device"),
                crate::utils::drive::DriveType::Nvme => i18n("NVMe Drive"),
                crate::utils::drive::DriveType::Unknown => i18n("N/A"),
                crate::utils::drive::DriveType::Raid => i18n("Software Raid"),
                crate::utils::drive::DriveType::RamDisk => i18n("RAM Disk"),
                crate::utils::drive::DriveType::Ssd => i18n("Solid State Drive"),
                crate::utils::drive::DriveType::ZfsVolume => i18n("ZFS Volume"),
                crate::utils::drive::DriveType::Zram => i18n("Compressed RAM Disk (zram)"),
            }),
        );

        imp.device.set_subtitle(&drive.block_device);

        imp.last_timestamp.set(
            SystemTime::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap(),
        );
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

        if writable {
            imp.writable.set_subtitle(&i18n("Yes"));
        } else {
            imp.writable.set_subtitle(&i18n("No"));
        }

        if removable {
            imp.removable.set_subtitle(&i18n("Yes"));
        } else {
            imp.removable.set_subtitle(&i18n("No"));
        }

        let total_usage = if let (
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

        let percentage_string = format!("{} %", (total_usage.unwrap_or(0.0) * 100.0).round());
        imp.total_usage.push_data_point(total_usage.unwrap_or(0.0));
        imp.total_usage.set_subtitle(&percentage_string);
        self.set_property("usage", total_usage.unwrap_or(0.0));

        let rw_bytes_per_second = if let (
            Some(read_sectors),
            Some(write_sectors),
            Some(old_read_sectors),
            Some(old_write_sectors),
        ) = (
            disk_stats.get("read_sectors"),
            disk_stats.get("write_sectors"),
            imp.old_stats.borrow().get("read_sectors"),
            imp.old_stats.borrow().get("write_sectors"),
        ) {
            let delta_read_sectors = read_sectors.saturating_sub(*old_read_sectors);
            let delta_write_sectors = write_sectors.saturating_sub(*old_write_sectors);
            Some((
                (delta_read_sectors * 512) as f64 / time_passed,
                (delta_write_sectors * 512) as f64 / time_passed,
            ))
        } else {
            None
        };

        let formatted_read_speed =
            convert_speed(rw_bytes_per_second.unwrap_or((0.0, 0.0)).0, false);
        let formatted_write_speed =
            convert_speed(rw_bytes_per_second.unwrap_or((0.0, 0.0)).1, false);

        imp.read.set_subtitle(&formatted_read_speed);
        imp.write.set_subtitle(&formatted_write_speed);

        self.set_property(
            "tab_subtitle",
            // Translators: This is an abbreviation for "Read" and "Write". This is displayed in the sidebar so your
            // translation should preferably be quite short or an abbreviation
            i18n_f(
                "R: {} · W: {}",
                &[&formatted_read_speed, &formatted_write_speed],
            ),
        );

        imp.capacity
            .set_subtitle(&convert_storage(capacity as f64, false));

        *imp.old_stats.borrow_mut() = disk_stats;
        imp.last_timestamp.set(SystemTime::now());
    }
}
