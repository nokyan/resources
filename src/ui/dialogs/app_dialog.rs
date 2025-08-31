use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::pages::applications::application_entry::ApplicationEntry;
use crate::utils::units::{convert_speed, convert_storage};
use adw::{prelude::*, subclass::prelude::*};
use gtk::gio::ThemedIcon;
use gtk::glib;
use log::trace;

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
        pub swap_usage: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_read_speed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_read_total: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_write_speed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub drive_write_total: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub gpu_usage: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub vram_usage: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub encoder_usage: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub decoder_usage: TemplateChild<adw::ActionRow>,
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
        type ParentType = adw::Dialog;

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
    impl AdwDialogImpl for ResAppDialog {}
}

glib::wrapper! {
    pub struct ResAppDialog(ObjectSubclass<imp::ResAppDialog>)
        @extends gtk::Widget, adw::Dialog, gtk::Window,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Accessible, gtk::ShortcutManager, gtk::Root,
        gtk::Native;
}

impl Default for ResAppDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ResAppDialog {
    pub fn new() -> Self {
        trace!("Creating ResAppDialog GObject…");

        glib::Object::new::<Self>()
    }

    pub fn init(&self, app: &ApplicationEntry) {
        self.setup_widgets(app);
    }

    pub fn setup_widgets(&self, app: &ApplicationEntry) {
        trace!("Setting up ResAppDialog widgets…");

        let imp = self.imp();

        if app.id().is_none() // this will be the case for System Processes
            || app
                .icon()
                .downcast_ref::<ThemedIcon>()
                .is_some_and(|themed_icon| {
                    themed_icon
                        .names()
                        .iter()
                        .all(|name| name.ends_with("-symbolic"))
                        || themed_icon
                            .names()
                            .iter()
                            .all(|name| name.contains("generic-process"))
                })
        {
            imp.icon.set_pixel_size(imp.icon.pixel_size() / 2);
            imp.icon.set_css_classes(&["big-bubble"]);
        }

        imp.icon.set_from_gicon(&app.icon());

        imp.name.set_label(&app.name());

        if let Some(description) = &app.description() {
            imp.description.set_label(description);
        } else {
            imp.description.set_visible(false);
        }

        if let Some(id) = &app.id() {
            imp.id.set_subtitle(id);
        } else {
            imp.id.set_visible(false);
        }

        imp.running_since
            .set_subtitle(&app.running_since().unwrap_or_else(|| i18n("N/A").into()));

        imp.containerized.set_subtitle(&app.containerization());

        self.update(app);
    }

    pub fn update(&self, app: &ApplicationEntry) {
        trace!("Refreshing ResAppDialog…");

        let imp = self.imp();

        imp.cpu_usage
            .set_subtitle(&format!("{:.1} %", app.cpu_usage() * 100.0));

        imp.memory_usage
            .set_subtitle(&convert_storage(app.memory_usage() as f64, false));

        imp.swap_usage
            .set_subtitle(&convert_storage(app.swap_usage() as f64, false));

        imp.drive_read_speed
            .set_subtitle(&convert_speed(app.read_speed(), false));

        imp.drive_read_total
            .set_subtitle(&convert_storage(app.read_total() as f64, false));

        imp.drive_write_speed
            .set_subtitle(&convert_speed(app.write_speed(), false));

        imp.drive_write_total
            .set_subtitle(&convert_storage(app.write_total() as f64, false));

        imp.gpu_usage
            .set_subtitle(&format!("{:.1} %", app.gpu_usage() * 100.0));

        imp.vram_usage
            .set_subtitle(&convert_storage(app.gpu_mem_usage() as f64, false));

        imp.encoder_usage
            .set_subtitle(&format!("{:.1} %", app.enc_usage() * 100.0));

        imp.decoder_usage
            .set_subtitle(&format!("{:.1} %", app.dec_usage() * 100.0));

        imp.processes_amount
            .set_subtitle(&app.running_processes().to_string());
    }
}
