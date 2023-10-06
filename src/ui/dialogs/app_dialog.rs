use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::window::MainWindow;
use crate::utils::processes::{AppItem, Containerization};
use crate::utils::units::convert_storage;

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
        pub id: TemplateChild<adw::ActionRow>,
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
        let imp = self.imp();

        imp.icon.set_gicon(Some(&app.icon));

        imp.name.set_label(&app.display_name);

        self.set_cpu_usage(app.cpu_time_ratio);

        self.set_memory_usage(app.memory_usage);

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

        self.set_processes_amount(app.processes_amount);

        let containerized = match app.containerization {
            Containerization::None => i18n("No"),
            Containerization::Flatpak => i18n("Yes (Flatpak)"),
        };
        imp.containerized.set_subtitle(&containerized);
    }

    pub fn set_cpu_usage(&self, usage: f32) {
        let imp = self.imp();
        imp.cpu_usage
            .set_subtitle(&format!("{:.1} %", usage * 100.0));
    }

    pub fn set_memory_usage(&self, usage: usize) {
        let imp = self.imp();
        imp.memory_usage
            .set_subtitle(&convert_storage(usage as f64, false));
    }

    pub fn set_processes_amount(&self, amount: usize) {
        let imp = self.imp();
        imp.processes_amount.set_subtitle(&amount.to_string());
    }
}
