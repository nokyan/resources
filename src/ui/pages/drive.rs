use std::time::SystemTime;

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, timeout_future_seconds, MainContext};

use crate::config::PROFILE;
use crate::ui::widgets::info_box::ResInfoBox;
use crate::utils;
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use std::cell::RefCell;

    use crate::ui::widgets::{bool_box::ResBoolBox, graph_box::ResGraphBox};

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/pages/drive.ui")]
    pub struct ResDrive {
        #[template_child]
        pub drive_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub total_usage: TemplateChild<ResGraphBox>,
        #[template_child]
        pub read: TemplateChild<ResInfoBox>,
        #[template_child]
        pub write: TemplateChild<ResInfoBox>,
        #[template_child]
        pub device: TemplateChild<ResInfoBox>,
        #[template_child]
        pub capacity: TemplateChild<ResInfoBox>,
        #[template_child]
        pub writable: TemplateChild<ResBoolBox>,
        #[template_child]
        pub removable: TemplateChild<ResBoolBox>,
        pub last_checked_timestamp: RefCell<u64>,
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
            let obj = self.instance();

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

    pub fn init(
        &self,
        vendor: &str,
        model: &str,
        device: &str,
        capacity: u64,
        writable: bool,
        removable: bool,
    ) {
        let imp = self.imp();
        *imp.last_checked_timestamp.borrow_mut() = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis() as u64);
        self.setup_widgets(vendor, model, device, capacity, writable, removable);
        self.setup_listener(device.replace("/dev/", ""));
    }

    pub fn setup_widgets(
        &self,
        vendor: &str,
        model: &str,
        device: &str,
        capacity: u64,
        writable: bool,
        removable: bool,
    ) {
        let imp = self.imp();
        imp.total_usage.set_title_label("Total Usage");
        imp.total_usage.set_data_points_max_amount(60);
        imp.total_usage.set_graph_color(229, 165, 10);
        imp.drive_name.set_label(&vec![vendor, model].join(" "));
        imp.device.set_info_label(device);
        let formatted_capacity = to_largest_unit(capacity as f64, &Base::Decimal);
        imp.capacity.set_info_label(&format!(
            "{:.1} {}B",
            formatted_capacity.0, formatted_capacity.1
        ));
        imp.writable.set_bool(writable);
        imp.removable.set_bool(removable);
    }

    pub fn setup_listener(&self, device: String) {
        let main_context = MainContext::default();
        let drive_usage_update = clone!(@strong self as this => async move {
            let hw_sector_size = utils::drive::get_sector_size(&device).await.unwrap_or(512);
            let mut old_stats = utils::drive::sys_stat(&device).await.unwrap_or_default();
            // TODO: make this maybe configurable?
            let refresh_seconds: u32 = 1;
            let imp = this.imp();
            
            let mut last_checked_timestamp = *imp.last_checked_timestamp.borrow_mut();
            
            loop {
                let disk_stats = utils::drive::sys_stat(&device).await.unwrap_or_default();
                
                let time_passed_millis = (SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    // if that for some reason doesn't work, just assume that
                    // exactly 1000ms did actually pass
                    .map_or(1000, |duration| duration.as_millis() as u64)
                    - last_checked_timestamp) as f64;

                if let (Some(read_ticks), Some(write_ticks), Some(old_read_ticks), Some(old_write_ticks)) = (disk_stats.get("read_ticks"), disk_stats.get("write_ticks"), old_stats.get("read_ticks"), old_stats.get("write_ticks")) {
                    let delta_read_ticks = read_ticks - old_read_ticks;
                    let delta_write_ticks = write_ticks - old_write_ticks;
                    let read_ratio = delta_read_ticks as f64 / time_passed_millis;
                    let write_ratio = delta_write_ticks as f64 / time_passed_millis;
                    let percentage = f64::max(read_ratio, write_ratio).clamp(0.0, 1.0);
                    let percentage_string = format!("{} %", (percentage * 100.0) as u8);
                    imp.total_usage.push_data_point(percentage);
                    imp.total_usage.set_info_label(&percentage_string);
                }

                if let (Some(read_sectors), Some(write_sectors), Some(old_read_sectors), Some(old_write_sectors)) = (disk_stats.get("read_sectors"), disk_stats.get("write_sectors"), old_stats.get("read_sectors"), old_stats.get("write_sectors")) {
                    let delta_read_sectors = read_sectors - old_read_sectors;
                    let delta_write_sectors = write_sectors - old_write_sectors;
                    let read_bytes_per_second = (delta_read_sectors * hw_sector_size) as f64 / time_passed_millis * 1000.0;
                    let write_bytes_per_second = (delta_write_sectors * hw_sector_size) as f64 / time_passed_millis * 1000.0;
                    let rbps_formatted = to_largest_unit(read_bytes_per_second, &Base::Decimal);
                    let wbps_formatted = to_largest_unit(write_bytes_per_second, &Base::Decimal);
                    imp.read.set_info_label(&format!("{:.2} {}B/s", rbps_formatted.0, rbps_formatted.1));
                    imp.write.set_info_label(&format!("{:.2} {}B/s", wbps_formatted.0, wbps_formatted.1));
                }

                old_stats = disk_stats;
                last_checked_timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                // if that for some reason doesn't work, just assume that
                // exactly 1000ms did actually pass
                .map_or_else(|_| last_checked_timestamp + (refresh_seconds as u64) * 1000, |duration| duration.as_millis() as u64);

                timeout_future_seconds(refresh_seconds).await;
            }
        });
        main_context.spawn_local(drive_usage_update);
    }

    pub fn set_writable(&self, writable: bool) {
        let imp = self.imp();
        imp.writable.set_bool(writable);
    }

    pub fn set_capacity(&self, capacity: u64) {
        let imp = self.imp();
        let capacity_formatted = to_largest_unit(capacity as f64, &Base::Decimal);
        imp.capacity.set_info_label(&format!(
            "{:.1} {}B",
            capacity_formatted.0, capacity_formatted.1
        ));
    }
}
