use log::{debug, info};

use adw::{prelude::*, subclass::prelude::*};
use glib::clone;
use gtk::{gdk, gio, glib};

use crate::config::{self, APP_ID, PKGDATADIR, PROFILE, VERSION};
use crate::i18n::i18n;
use crate::ui::dialogs::settings_dialog::ResSettingsDialog;
use crate::ui::window::MainWindow;
use crate::utils::os::OsInfo;
use crate::utils::process::ProcessAction;

mod imp {
    use std::{cell::Cell, sync::OnceLock};

    use super::*;
    use glib::WeakRef;

    #[derive(Debug, Default)]
    pub struct Application {
        pub window: OnceLock<WeakRef<MainWindow>>,

        pub settings_window_opened: Cell<bool>,
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
        action_quit.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                // This is needed to trigger the delete event and saving the window state
                this.main_window().close();
                this.quit();
            }
        ));
        self.add_action(&action_quit);

        // Toggle Search
        let action_search = gio::SimpleAction::new("toggle-search", None);
        action_search.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.main_window().shortcut_toggle_search();
            }
        ));
        self.add_action(&action_search);

        // Show Settings
        let action_settings = gio::SimpleAction::new("settings", None);
        action_settings.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.show_settings_dialog();
            }
        ));
        self.add_action(&action_settings);

        // About
        let action_about = gio::SimpleAction::new("about", None);
        action_about.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.show_about_dialog();
            }
        ));
        self.add_action(&action_about);

        // End App/Process
        let action_end_app_process = gio::SimpleAction::new("end-app-process", None);
        action_end_app_process.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.main_window()
                    .shortcut_manipulate_app_process(ProcessAction::TERM);
            }
        ));
        self.add_action(&action_end_app_process);

        // Kill App/Process
        let action_kill_app_process = gio::SimpleAction::new("kill-app-process", None);
        action_kill_app_process.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.main_window()
                    .shortcut_manipulate_app_process(ProcessAction::KILL);
            }
        ));
        self.add_action(&action_kill_app_process);

        // Halt App/Process
        let action_halt_app_process = gio::SimpleAction::new("halt-app-process", None);
        action_halt_app_process.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.main_window()
                    .shortcut_manipulate_app_process(ProcessAction::STOP);
            }
        ));
        self.add_action(&action_halt_app_process);

        // Continue App/Process
        let action_continue_app_process = gio::SimpleAction::new("continue-app-process", None);
        action_continue_app_process.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.main_window()
                    .shortcut_manipulate_app_process(ProcessAction::CONT);
            }
        ));
        self.add_action(&action_continue_app_process);

        // Show Information for App/Process
        let action_information_app_process =
            gio::SimpleAction::new("information-app-process", None);
        action_information_app_process.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.main_window().shortcut_information_app_process();
            }
        ));
        self.add_action(&action_information_app_process);

        // Show Process Options
        let action_process_options = gio::SimpleAction::new("process-options", None);
        action_process_options.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.main_window().shortcut_process_options();
            }
        ));
        self.add_action(&action_process_options);
    }

    // Sets up keyboard shortcuts
    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q", "<Control>w"]);
        self.set_accels_for_action("app.settings", &["<Control>comma"]);
        self.set_accels_for_action("app.toggle-search", &["<Control>f", "F3"]);
        self.set_accels_for_action("app.end-app-process", &["<Control>E", "Delete"]);
        self.set_accels_for_action("app.kill-app-process", &["<Control>K", "<Shift>Delete"]);
        self.set_accels_for_action("app.halt-app-process", &["<Control>H"]);
        self.set_accels_for_action("app.continue-app-process", &["<Control>N"]);
        self.set_accels_for_action("app.information-app-process", &["<Control>I"]);
        self.set_accels_for_action("app.process-options", &["<Control>O"]);
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
        let imp = self.imp();

        let settings_window_opened = imp.settings_window_opened.take();
        imp.settings_window_opened.set(settings_window_opened);
        if settings_window_opened {
            return;
        }

        let settings = ResSettingsDialog::new();

        settings.init();

        settings.present(Some(&self.main_window()));
        imp.settings_window_opened.set(true);

        settings.connect_closed(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.imp().settings_window_opened.set(false);
            }
        ));
    }

    fn show_about_dialog(&self) {
        let about = adw::AboutDialog::builder()
            .application_name(i18n("Resources"))
            .application_icon(config::APP_ID)
            .developer_name(i18n("The Nalux Team"))
            .developers(vec!["nokyan <hello@nokyan.net>"])
            .license_type(gtk::License::Gpl30)
            .version(config::VERSION)
            .website("https://apps.gnome.org/app/net.nokyan.Resources/")
            .build();

        about.add_link(
            &i18n("Report Issues"),
            "https://github.com/nokyan/resources/issues",
        );

        // Translator credits. Replace "translator-credits" with your name/username, and optionally an email or URL.
        // One name per line, please do not remove previous names.
        about.set_translator_credits(&i18n("translator-credits"));
        about.add_credit_section(Some(&i18n("Icon by")), &["Avhiren"]);

        about.present(Some(&self.main_window()));
    }

    pub fn run(&self) {
        info!("Resources ({APP_ID})");
        info!("Version: {VERSION}");
        info!("Datadir: {PKGDATADIR}");

        let os_info = OsInfo::get();
        debug!(
            "Operating system: {}",
            os_info.name.as_deref().unwrap_or("N/A")
        );
        debug!(
            "Kernel version: {}",
            os_info.kernel_version.as_deref().unwrap_or("N/A")
        );

        if PROFILE == "Devel" {
            info!(
                "You are running a development version of Resources, things may be slow or break!"
            );
        }

        ApplicationExtManual::run_with_args::<&str>(self, &[]);
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
