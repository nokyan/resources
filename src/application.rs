use log::{debug, info};

use adw::{prelude::*, subclass::prelude::*};
use glib::clone;
use gtk::{gdk, gio, glib};

use crate::config::{self, APP_ID, PKGDATADIR, PROFILE, VERSION};
use crate::i18n::i18n;
use crate::ui::dialogs::settings_dialog::ResSettingsDialog;
use crate::ui::window::MainWindow;

mod imp {
    use std::sync::OnceLock;

    use super::*;
    use glib::WeakRef;

    #[derive(Debug, Default)]
    pub struct Application {
        pub window: OnceLock<WeakRef<MainWindow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "Application";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self) {
            debug!("GtkApplication<Application>::activate");
            self.parent_activate();
            let app = self.obj();

            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.present();
                return;
            }

            let window = MainWindow::new(&app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            app.main_window().present();
        }

        fn startup(&self) {
            debug!("GtkApplication<Application>::startup");
            self.parent_startup();
            let app = self.obj();

            // Set icons for shell
            gtk::Window::set_default_icon_name(APP_ID);

            app.setup_css();
            app.setup_gactions();
            app.setup_accels();
        }
    }

    impl GtkApplicationImpl for Application {}

    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::builder::<Self>()
            .property("application-id", Some(APP_ID))
            .property("flags", gio::ApplicationFlags::empty())
            .property("resource-base-path", Some("/net/nokyan/Resources/"))
            .build()
    }

    fn main_window(&self) -> MainWindow {
        self.imp().window.get().unwrap().upgrade().unwrap()
    }

    fn setup_gactions(&self) {
        // Quit
        let action_quit = gio::SimpleAction::new("quit", None);
        action_quit.connect_activate(clone!(@weak self as app => move |_, _| {
            // This is needed to trigger the delete event and saving the window state
            app.main_window().close();
            app.quit();
        }));
        self.add_action(&action_quit);

        // About
        let action_settings = gio::SimpleAction::new("settings", None);
        action_settings.connect_activate(clone!(@weak self as app => move |_, _| {
            app.show_settings_dialog();
        }));
        self.add_action(&action_settings);

        // About
        let action_about = gio::SimpleAction::new("about", None);
        action_about.connect_activate(clone!(@weak self as app => move |_, _| {
            app.show_about_dialog();
        }));
        self.add_action(&action_about);
    }

    // Sets up keyboard shortcuts
    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
    }

    fn setup_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/net/nokyan/Resources/style.css");
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn show_settings_dialog(&self) {
        let settings = ResSettingsDialog::new();

        settings.set_transient_for(Some(&self.main_window()));
        settings.set_modal(true);

        settings.init();

        settings.present();
    }

    fn show_about_dialog(&self) {
        let about = adw::AboutWindow::builder()
            .application_name(i18n("Resources"))
            .application_icon(config::APP_ID)
            .developer_name(i18n("The Nalux Team"))
            .developers(vec!["nokyan <nokyan@tuta.io>".to_string()])
            .license_type(gtk::License::Gpl30)
            .version(config::VERSION)
            .website("https://github.com/nokyan/resources")
            .build();

        about.add_link(
            &i18n("Report Issues"),
            "https://github.com/nokyan/resources/issues",
        );

        about.add_credit_section(Some(&i18n("Icon by")), &["Avhiren"]);

        about.set_transient_for(Some(&self.main_window()));
        about.set_modal(true);

        about.present();
    }

    pub fn run(&self) {
        info!("Resources ({})", APP_ID);
        info!("Version: {} ({})", VERSION, PROFILE);
        info!("Datadir: {}", PKGDATADIR);

        ApplicationExtManual::run(self);
    }
}

impl Default for Application {
    fn default() -> Self {
        gio::Application::default()
            .expect("Could not get default GApplication")
            .downcast()
            .unwrap()
    }
}
