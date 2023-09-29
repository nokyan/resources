use std::time::{Duration, SystemTime};

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, timeout_future_seconds, MainContext};

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::utils::drive::Drive;
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use crate::ui::widgets::graph_box::ResGraphBox;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/pages/drive.ui")]
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
        self.setup_widgets(drive.clone());
        self.setup_listener(drive);
    }

    pub fn setup_widgets(&self, drive: Drive) {
        let main_context = MainContext::default();
        let drive_stats = clone!(@strong self as this => async move {
            let imp = this.imp();
            imp.total_usage.set_title_label("Total Usage");
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
            let formatted_capacity =
                to_largest_unit((drive.capacity().await.unwrap_or(0) * drive.sector_size().await.unwrap_or(512)) as f64, &Base::Decimal);
            imp.capacity.set_subtitle(&format!(
                "{:.1} {}B",
                formatted_capacity.0, formatted_capacity.1
            ));

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
                    let read_ratio = delta_read_ticks as f64 / time_passed;
                    let write_ratio = delta_write_ticks as f64 / time_passed;
                    let percentage = f64::max(read_ratio, write_ratio).clamp(0.0, 1.0);
                    let percentage_string = format!("{} %", (percentage * 100.0) as u8);
                    imp.total_usage.push_data_point(percentage);
                    imp.total_usage.set_subtitle(&percentage_string);
                }

                if let (Some(read_sectors), Some(write_sectors), Some(old_read_sectors), Some(old_write_sectors)) = (disk_stats.get("read_sectors"), disk_stats.get("write_sectors"), old_stats.get("read_sectors"), old_stats.get("write_sectors")) {
                    let delta_read_sectors = read_sectors - old_read_sectors;
                    let delta_write_sectors = write_sectors - old_write_sectors;
                    let read_bytes_per_second = (delta_read_sectors * hw_sector_size) as f64 / time_passed * 1000.0;
                    let write_bytes_per_second = (delta_write_sectors * hw_sector_size) as f64 / time_passed * 1000.0;
                    let rbps_formatted = to_largest_unit(read_bytes_per_second, &Base::Decimal);
                    let wbps_formatted = to_largest_unit(write_bytes_per_second, &Base::Decimal);
                    imp.read.set_subtitle(&format!("{:.2} {}B/s", rbps_formatted.0, rbps_formatted.1));
                    imp.write.set_subtitle(&format!("{:.2} {}B/s", wbps_formatted.0, wbps_formatted.1));
                }

                let formatted_capacity =
                    to_largest_unit((drive.capacity().await.unwrap_or(0) * hw_sector_size as u64) as f64, &Base::Decimal);
                imp.capacity.set_subtitle(&format!(
                    "{:.1} {}B",
                    formatted_capacity.0, formatted_capacity.1
                ));

                old_stats = disk_stats;
                last_timestamp = SystemTime::now();

                timeout_future_seconds(1).await;
            }
        });
        main_context.spawn_local(drive_usage_update);
    }

    pub fn set_writable(&self, writable: bool) {
        let imp = self.imp();
        if writable {
            imp.writable.set_subtitle(&i18n("Yes"));
        } else {
            imp.writable.set_subtitle(&i18n("No"));
        }
    }

    pub fn set_capacity(&self, capacity: u64) {
        let imp = self.imp();
        let capacity_formatted = to_largest_unit(capacity as f64, &Base::Decimal);
        imp.capacity.set_subtitle(&format!(
            "{:.1} {}B",
            capacity_formatted.0, capacity_formatted.1
        ));
    }
}
