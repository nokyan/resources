use crate::{
    config::PROFILE,
    i18n::i18n_f,
    ui::pages::{processes::process_entry::ProcessEntry, NICE_TO_LABEL},
    utils::settings::SETTINGS,
};
use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;
use process_data::Niceness;

mod imp {

    use std::cell::Cell;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/net/nokyan/Resources/ui/dialogs/process_options_dialog.ui")]
    pub struct ResProcessOptionsDialog {
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
        #[template_child]
        pub apply_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub nice_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub priority_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub affinity_row: TemplateChild<adw::ExpanderRow>,

        pub pid: Cell<libc::pid_t>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResProcessOptionsDialog {
        const NAME: &'static str = "ResProcessOptionsDialog";
        type Type = super::ResPriorityDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResProcessOptionsDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResProcessOptionsDialog {}
    impl WindowImpl for ResProcessOptionsDialog {}
    impl AdwDialogImpl for ResProcessOptionsDialog {}
}

glib::wrapper! {
    pub struct ResPriorityDialog(ObjectSubclass<imp::ResProcessOptionsDialog>)
        @extends gtk::Widget, adw::Dialog;
}

impl Default for ResPriorityDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ResPriorityDialog {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, process: &ProcessEntry) {
        self.setup_widgets(process);
    }

    pub fn setup_widgets(&self, process: &ProcessEntry) {
        let imp = self.imp();

        imp.name.set_label(&process.name());

        imp.nice_row.set_value(process.niceness() as f64);

        imp.priority_row.set_selected(
            NICE_TO_LABEL
                .get(&Niceness::try_from(process.niceness()).unwrap_or_default())
                .map(|(_, i)| *i)
                .unwrap_or(2),
        );

        if SETTINGS.detailed_priority() {
            imp.priority_row.set_visible(false);
        } else {
            imp.nice_row.set_visible(false);
        }

        for (i, affinity) in process.affinity().iter().enumerate() {
            let switch_row = adw::SwitchRow::builder()
                .title(&i18n_f("CPU {}", &[&(i + 1).to_string()]))
                .active(*affinity)
                .build();
            imp.affinity_row.add_row(&switch_row);
        }

        imp.pid.set(process.pid())
    }
}
