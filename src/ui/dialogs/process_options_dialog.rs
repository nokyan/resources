use crate::{
    config::PROFILE,
    i18n::{i18n, i18n_f},
    ui::{
        pages::{processes::process_entry::ProcessEntry, NICE_TO_LABEL},
        window::{Action, MainWindow},
    },
    utils::settings::SETTINGS,
};
use adw::{prelude::*, subclass::prelude::*, ToastOverlay};
use async_channel::Sender;
use gtk::glib::{self, clone, MainContext};
use process_data::Niceness;

mod imp {

    use std::cell::{Cell, RefCell};

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

        pub current_affinity: RefCell<Vec<bool>>,

        pub pid: Cell<libc::pid_t>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResProcessOptionsDialog {
        const NAME: &'static str = "ResProcessOptionsDialog";
        type Type = super::ResProcessOptionsDialog;
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
    pub struct ResProcessOptionsDialog(ObjectSubclass<imp::ResProcessOptionsDialog>)
        @extends gtk::Widget, adw::Dialog;
}

impl Default for ResProcessOptionsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ResProcessOptionsDialog {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(
        &self,
        process: &ProcessEntry,
        sender: Sender<Action>,
        toast_overlay: &ToastOverlay,
    ) {
        self.setup_widgets(process);
        self.setup_signals(process, sender, toast_overlay)
    }

    fn get_current_niceness(&self) -> Niceness {
        let imp = self.imp();

        if imp.priority_row.is_visible() {
            match imp.priority_row.selected() {
                0 => Niceness::try_from(20).unwrap_or_default(),
                1 => Niceness::try_from(5).unwrap_or_default(),
                2 => Niceness::try_from(0).unwrap_or_default(),
                3 => Niceness::try_from(5).unwrap_or_default(),
                4 => Niceness::try_from(19).unwrap_or_default(),
                _ => Niceness::default(),
            }
        } else {
            Niceness::try_from(imp.nice_row.value() as i8).unwrap_or_default()
        }
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

        *imp.current_affinity.borrow_mut() = process.affinity();

        for (i, affinity) in process.affinity().iter().enumerate() {
            let switch_row = adw::SwitchRow::builder()
                .title(&i18n_f("CPU {}", &[&(i + 1).to_string()]))
                .active(*affinity)
                .build();

            switch_row.connect_active_notify(clone!(
                #[weak(rename_to = this)]
                self,
                move |switch_row| {
                    // TODO: maybe not a direct index access?
                    this.imp().current_affinity.borrow_mut()[i] = switch_row.is_active();
                }
            ));

            imp.affinity_row.add_row(&switch_row);
        }

        imp.pid.set(process.pid())
    }

    pub fn setup_signals(
        &self,
        process: &ProcessEntry,
        sender: Sender<Action>,
        toast_overlay: &ToastOverlay,
    ) {
        self.imp().apply_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            process,
            #[weak]
            toast_overlay,
            #[strong]
            sender,
            move |_| {
                let main_context = MainContext::default();
                main_context.spawn_local(clone!(
                    #[weak]
                    this,
                    #[strong]
                    sender,
                    async move {
                        let imp = this.imp();

                        if !imp.current_affinity.borrow().iter().any(|b| *b) {
                            let dialog = adw::MessageDialog::new(
                                Some(&MainWindow::default()),
                                Some(&i18n("Unable to Apply Adjustment")),
                                Some(&i18n("Please ensure that at least one CPU is enabled in the processor affinity."))
                            );

                            dialog.add_response("OK", &i18n("OK"));
                            dialog.set_default_response(Some("OK"));
                            dialog.present();

                            return;
                        }

                        let _ = sender
                            .send(Action::AdjustProcess(
                                process.pid(),
                                this.get_current_niceness(),
                                imp.current_affinity.borrow().clone(),
                                process.name().to_string(),
                                toast_overlay.clone(),
                            ))
                            .await;
                    }
                ));
            }
        ));
    }
}
