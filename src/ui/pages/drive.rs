use std::time::{Duration, SystemTime};

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, timeout_future, MainContext};

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::utils::drive::Drive;
use crate::utils::settings::SETTINGS;
use crate::utils::units::{convert_speed, convert_storage};

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::widgets::graph_box::ResGraphBox;

    use super::*;

    use gtk::{
        gio::Icon,
        glib::{ParamSpec, Properties, Value},
        CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/me/nalux/Resources/ui/pages/drive.ui")]
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

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: RefCell<Icon>,

        #[property(get, set)]
        usage: Cell<f64>,

        #[property(get = Self::tab_name, set = Self::set_tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,
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

    pub fn init(&self, drive: Drive) {
        self.imp().set_icon(&drive.icon());
        self.setup_widgets(drive.clone());
        self.setup_listener(drive);
    }

    pub fn setup_widgets(&self, drive: Drive) {
        let main_context = MainContext::default();
        let drive_stats = clone!(@strong self as this => async move {
            let imp = this.imp();
            imp.total_usage.set_title_label(&i18n("Total Usage"));
            imp.total_usage.set_data_points_max_amount(60);
            imp.total_usage.set_graph_color(229, 165, 10);
            imp.drive_type.set_subtitle(&(match drive.drive_type {
                crate::utils::drive::DriveType::CdDvdBluray => i18n("CD/DVD/Blu-ray Drive"),
                crate::utils::drive::DriveType::Emmc => i18n("eMMC Storage"),
                crate::utils::drive::DriveType::Flash => i18n("Flash Storage"),
                crate::utils::drive::DriveType::Floppy => i18n("Floppy Drive"),
                crate::utils::drive::DriveType::Hdd => i18n("Hard Disk Drive"),
                crate::utils::drive::DriveType::Nvme => i18n("NVMe Drive"),
                crate::utils::drive::DriveType::Unknown => i18n("N/A"),
                crate::utils::drive::DriveType::Ssd => i18n("Solid State Drive"),
            }));
            imp.device.set_subtitle(&drive.block_device);

            let capacity = drive.capacity().await.unwrap_or(0) * drive.sector_size().await.unwrap_or(512);
            imp.capacity.set_subtitle(&convert_storage(capacity as f64, false));

            if drive.writable().await.unwrap_or(false) {
                imp.writable.set_subtitle(&i18n("Yes"));
            } else {
                imp.writable.set_subtitle(&i18n("No"));
            }

            if drive.removable().await.unwrap_or(false) {
                imp.removable.set_subtitle(&i18n("Yes"));
            } else {
                imp.removable.set_subtitle(&i18n("No"));
            }
        });
        main_context.spawn_local(drive_stats);
    }

    pub fn setup_listener(&self, drive: Drive) {
        let main_context = MainContext::default();
        let drive_usage_update = clone!(@strong self as this => async move {
            let hw_sector_size = drive.sector_size().await.unwrap_or(512) as usize;
            let mut old_stats = drive.sys_stats().await.unwrap_or_default();
            let imp = this.imp();

            let mut last_timestamp = SystemTime::now().checked_sub(Duration::from_secs(1)).unwrap();

            loop {
                let disk_stats = drive.sys_stats().await.unwrap_or_default();
                let time_passed = SystemTime::now().duration_since(last_timestamp).map_or(1.0f64, |timestamp| timestamp.as_secs_f64());

                if let (Some(read_ticks), Some(write_ticks), Some(old_read_ticks), Some(old_write_ticks)) = (disk_stats.get("read_ticks"), disk_stats.get("write_ticks"), old_stats.get("read_ticks"), old_stats.get("write_ticks")) {
                    let delta_read_ticks = read_ticks - old_read_ticks;
                    let delta_write_ticks = write_ticks - old_write_ticks;
                    let read_ratio = delta_read_ticks as f64 / (time_passed * 1000.0);
                    let write_ratio = delta_write_ticks as f64 / (time_passed * 1000.0);
                    let fraction = f64::max(read_ratio, write_ratio).clamp(0.0, 1.0);
                    let percentage_string = format!("{} %", (fraction * 100.0).round());
                    imp.total_usage.push_data_point(fraction);
                    imp.total_usage.set_subtitle(&percentage_string);
                    this.set_property("usage", fraction);
                }

                if let (Some(read_sectors), Some(write_sectors), Some(old_read_sectors), Some(old_write_sectors)) = (disk_stats.get("read_sectors"), disk_stats.get("write_sectors"), old_stats.get("read_sectors"), old_stats.get("write_sectors")) {
                    let delta_read_sectors = read_sectors - old_read_sectors;
                    let delta_write_sectors = write_sectors - old_write_sectors;
                    let read_bytes_per_second = (delta_read_sectors * hw_sector_size) as f64 / time_passed;
                    let write_bytes_per_second = (delta_write_sectors * hw_sector_size) as f64 / time_passed;
                    imp.read.set_subtitle(&convert_speed(read_bytes_per_second));
                    imp.write.set_subtitle(&convert_speed(write_bytes_per_second));
                }

                let capacity = drive.capacity().await.unwrap_or(0) * drive.sector_size().await.unwrap_or(512);
                imp.capacity.set_subtitle(&convert_storage(capacity as f64, false));

                old_stats = disk_stats;
                last_timestamp = SystemTime::now();

                timeout_future(Duration::from_secs_f32(SETTINGS.refresh_speed().ui_refresh_interval())).await;
            }
        });
        main_context.spawn_local(drive_usage_update);
    }
}
