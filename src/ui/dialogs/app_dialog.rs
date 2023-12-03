use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;
use process_data::Containerization;

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::window::MainWindow;
use crate::utils::app::AppItem;
use crate::utils::units::{convert_speed, convert_storage};

mod imp {

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/net/nokyan/Resources/ui/dialogs/app_dialog.ui")]
    pub struct ResAppDialog {
        #[template_child]
        pub icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
        #[template_child]
        pub description: TemplateChild<gtk::Label>,
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
        pub id: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub running_since: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub processes_amount: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub containerized: TemplateChild<adw::ActionRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResAppDialog {
        const NAME: &'static str = "ResAppDialog";
        type Type = super::ResAppDialog;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResAppDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResAppDialog {}
    impl WindowImpl for ResAppDialog {}
    impl AdwWindowImpl for ResAppDialog {}
}

glib::wrapper! {
    pub struct ResAppDialog(ObjectSubclass<imp::ResAppDialog>)
        @extends gtk::Widget, gtk::Window, adw::Window;
}

impl ResAppDialog {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, app: &AppItem) {
        self.set_transient_for(Some(&MainWindow::default()));
        self.setup_widgets(app);
    }

    pub fn setup_widgets(&self, app: &AppItem) {
        self.update(app);
    }

    pub fn update(&self, app: &AppItem) {
        let imp = self.imp();

        imp.icon.set_gicon(Some(&app.icon));

        imp.name.set_label(&app.display_name);

        if let Some(description) = &app.description {
            imp.description.set_label(description);
        } else {
            imp.description.set_visible(false);
        }

        if let Some(id) = &app.id {
            imp.id.set_subtitle(id);
        } else {
            imp.id.set_visible(false);
        }

        imp.running_since.set_subtitle(&app.running_since);

        imp.cpu_usage
            .set_subtitle(&format!("{:.1} %", app.cpu_time_ratio * 100.0));

        imp.memory_usage
            .set_subtitle(&convert_storage(app.memory_usage as f64, false));

        imp.drive_read_speed
            .set_subtitle(&convert_speed(app.read_speed, false));

        imp.drive_read_total
            .set_subtitle(&convert_storage(app.read_total as f64, false));

        imp.drive_write_speed
            .set_subtitle(&convert_speed(app.write_speed, false));

        imp.drive_write_total
            .set_subtitle(&convert_storage(app.write_total as f64, false));

        imp.processes_amount
            .set_subtitle(&app.processes_amount.to_string());

        let containerized = match app.containerization {
            Containerization::None => i18n("No"),
            Containerization::Flatpak => i18n("Yes (Flatpak)"),
            Containerization::Snap => i18n("Yes (Snap)"),
        };
        imp.containerized.set_subtitle(&containerized);
    }
}
