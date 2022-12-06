use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, MainContext};

use crate::config::PROFILE;
use crate::ui::widgets::progress_box::ResProgressBox;
use crate::ui::window::MainWindow;
use crate::utils::processes::{Containerization, SimpleItem};
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use crate::ui::widgets::info_box::ResInfoBox;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/dialogs/app_dialog.ui")]
    pub struct ResAppDialog {
        #[template_child]
        pub icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
        #[template_child]
        pub description: TemplateChild<gtk::Label>,
        #[template_child]
        pub cpu_usage: TemplateChild<ResInfoBox>,
        #[template_child]
        pub memory_usage: TemplateChild<ResInfoBox>,
        #[template_child]
        pub id: TemplateChild<ResInfoBox>,
        #[template_child]
        pub processes_amount: TemplateChild<ResInfoBox>,
        #[template_child]
        pub containerized: TemplateChild<ResInfoBox>,
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
            let obj = self.instance();

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
        glib::Object::new::<Self>(&[])
    }

    pub fn init(&self, app: &SimpleItem) {
        self.set_transient_for(Some(&MainWindow::default()));
        self.setup_widgets(app);
    }

    pub fn setup_widgets(&self, app: &SimpleItem) {
        let imp = self.imp();

        imp.icon.set_gicon(Some(&app.icon));

        imp.name.set_label(&app.display_name);

        imp.cpu_usage.set_info_label(&gettextrs::gettext("N/A"));

        imp.memory_usage.set_info_label(&gettextrs::gettext("N/A"));

        if let Some(description) = &app.description {
            imp.description.set_label(description);
        } else {
            imp.description.set_visible(false);
        }

        if let Some(id) = &app.id {
            imp.id.set_info_label(id);
        } else {
            imp.id.set_visible(false);
        }

        imp.processes_amount
            .set_info_label(&gettextrs::gettext("N/A"));

        let containerized = match app.containerization {
            Containerization::None => gettextrs::gettext("No"),
            Containerization::Flatpak => gettextrs::gettext("Yes (Flatpak)"),
        };
        imp.containerized.set_info_label(&containerized);
    }

    pub fn set_cpu_usage(&self, usage: f32) {
        let imp = self.imp();
        imp.cpu_usage
            .set_info_label(&format!("{:.1} %", usage * 100.0));
    }

    pub fn set_memory_usage(&self, usage: usize) {
        let imp = self.imp();
        let (number, prefix) = to_largest_unit(usage as f64, &Base::Decimal);
        imp.memory_usage
            .set_info_label(&format!("{number:.1} {prefix}B"));
    }

    pub fn set_processes_amount(&self, amount: usize) {
        let imp = self.imp();
        imp.processes_amount.set_info_label(&amount.to_string());
    }
}
