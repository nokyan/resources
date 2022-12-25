use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::window::MainWindow;
use crate::utils::processes::{Containerization, Process};
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use crate::ui::widgets::info_box::ResInfoBox;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/dialogs/process_dialog.ui")]
    pub struct ResProcessDialog {
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
        #[template_child]
        pub cpu_usage: TemplateChild<ResInfoBox>,
        #[template_child]
        pub memory_usage: TemplateChild<ResInfoBox>,
        #[template_child]
        pub pid: TemplateChild<ResInfoBox>,
        #[template_child]
        pub commandline: TemplateChild<ResInfoBox>,
        #[template_child]
        pub user: TemplateChild<ResInfoBox>,
        #[template_child]
        pub cgroup: TemplateChild<ResInfoBox>,
        #[template_child]
        pub containerized: TemplateChild<ResInfoBox>,
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
            let obj = self.instance();

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
        glib::Object::new::<Self>(&[])
    }

    pub fn init<S: AsRef<str>>(&self, process: &Process, user: S) {
        self.set_transient_for(Some(&MainWindow::default()));
        self.setup_widgets(process, user.as_ref());
    }

    pub fn setup_widgets(&self, process: &Process, user: &str) {
        let imp = self.imp();

        imp.name.set_label(&process.comm);

        imp.cpu_usage.set_info_label(&i18n("N/A"));

        imp.memory_usage.set_info_label(&i18n("N/A"));

        imp.pid.set_info_label(&process.pid.to_string());

        imp.commandline.set_info_label(&process.commandline);
        imp.commandline.set_tooltip(Some(&process.commandline));

        imp.user.set_info_label(user);

        imp.cgroup
            .set_info_label(&process.cgroup.clone().unwrap_or_else(|| i18n("N/A")));
        imp.cgroup
            .set_tooltip(Some(&process.cgroup.clone().unwrap_or_else(|| i18n("N/A"))));

        let containerized = match process.containerization {
            Containerization::None => i18n("No"),
            Containerization::Flatpak => i18n("Yes (Flatpak)"),
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
}
