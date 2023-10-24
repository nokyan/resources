use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;
use process_data::Containerization;

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::window::MainWindow;
use crate::utils::process::ProcessItem;
use crate::utils::units::{convert_speed, convert_storage};

mod imp {

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/net/nokyan/Resources/ui/dialogs/process_dialog.ui")]
    pub struct ResProcessDialog {
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
        #[template_child]
        pub cpu_usage: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub memory_usage: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_read_speed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_read_total: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_write_speed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_write_total: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub pid: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub commandline: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub user: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub cgroup: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub containerized: TemplateChild<adw::ActionRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResProcessDialog {
        const NAME: &'static str = "ResProcessDialog";
        type Type = super::ResProcessDialog;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResProcessDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResProcessDialog {}
    impl WindowImpl for ResProcessDialog {}
    impl AdwWindowImpl for ResProcessDialog {}
}

glib::wrapper! {
    pub struct ResProcessDialog(ObjectSubclass<imp::ResProcessDialog>)
        @extends gtk::Widget, gtk::Window, adw::Window;
}

impl ResProcessDialog {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init<S: AsRef<str>>(&self, process: &ProcessItem, user: S) {
        self.set_transient_for(Some(&MainWindow::default()));
        self.setup_widgets(process, user.as_ref());
    }

    pub fn setup_widgets(&self, process: &ProcessItem, user: &str) {
        let imp = self.imp();

        imp.user.set_subtitle(user);

        self.update(process);
    }

    pub fn update(&self, process: &ProcessItem) {
        let imp = self.imp();

        imp.name.set_label(&process.display_name);

        imp.cpu_usage
            .set_subtitle(&format!("{:.1} %", process.cpu_time_ratio * 100.0));

        imp.memory_usage
            .set_subtitle(&convert_storage(process.memory_usage as f64, false));

        if let Some(read_speed) = process.read_speed {
            imp.drive_read_speed
                .set_subtitle(&convert_speed(read_speed, false));
        } else {
            imp.drive_read_speed.set_subtitle(&i18n("N/A"));
        }

        if let Some(read_total) = process.read_total {
            imp.drive_read_total
                .set_subtitle(&convert_storage(read_total as f64, false));
        } else {
            imp.drive_read_total.set_subtitle(&i18n("N/A"));
        }

        if let Some(write_speed) = process.write_speed {
            imp.drive_write_speed
                .set_subtitle(&convert_speed(write_speed, false));
        } else {
            imp.drive_write_speed.set_subtitle(&i18n("N/A"));
        }

        if let Some(write_total) = process.write_total {
            imp.drive_write_total
                .set_subtitle(&convert_storage(write_total as f64, false));
        } else {
            imp.drive_write_total.set_subtitle(&i18n("N/A"));
        }

        imp.pid.set_subtitle(&process.pid.to_string());

        imp.commandline.set_subtitle(&process.commandline);
        imp.commandline.set_tooltip_text(Some(&process.commandline));

        imp.cgroup
            .set_subtitle(&process.cgroup.clone().unwrap_or_else(|| i18n("N/A")));
        imp.cgroup
            .set_tooltip_text(Some(&process.cgroup.clone().unwrap_or_else(|| i18n("N/A"))));

        let containerized = match process.containerization {
            Containerization::None => i18n("No"),
            Containerization::Flatpak => i18n("Yes (Flatpak)"),
        };
        imp.containerized.set_subtitle(&containerized);
    }
}
