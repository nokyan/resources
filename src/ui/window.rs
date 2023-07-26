use std::collections::HashMap;
use std::path::PathBuf;

use adw::{prelude::*, subclass::prelude::*};
use anyhow::Result;
use gtk::glib::{clone, timeout_future_seconds, MainContext};
use gtk::{gio, glib};

use crate::application::Application;
use crate::config::{APP_ID, PROFILE};
use crate::i18n::{i18n, i18n_f};
use crate::ui::pages::drive::ResDrive;
use crate::utils::drive::{Drive, DriveType};
use crate::utils::gpu::GPU;
use crate::utils::network::{InterfaceType, NetworkInterface};
use crate::utils::units::{to_largest_unit, Base};

use super::pages::gpu::ResGPU;
use super::pages::network::ResNetwork;

mod imp {
    use std::cell::RefCell;

    use crate::ui::pages::{
        applications::ResApplications, cpu::ResCPU, memory::ResMemory, network::ResNetwork,
        processes::ResProcesses,
    };

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/me/nalux/Resources/ui/window.ui")]
    pub struct MainWindow {
        #[template_child]
        pub flap: TemplateChild<adw::Flap>,
        #[template_child]
        pub resources_sidebar: TemplateChild<gtk::StackSidebar>,
        #[template_child]
        pub content_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub cpu: TemplateChild<ResCPU>,
        #[template_child]
        pub cpu_page: TemplateChild<gtk::StackPage>,
        #[template_child]
        pub applications: TemplateChild<ResApplications>,
        #[template_child]
        pub applications_page: TemplateChild<gtk::StackPage>,
        #[template_child]
        pub processes: TemplateChild<ResProcesses>,
        #[template_child]
        pub processes_page: TemplateChild<gtk::StackPage>,
        #[template_child]
        pub memory: TemplateChild<ResMemory>,
        #[template_child]
        pub memory_page: TemplateChild<gtk::StackPage>,

        pub drive_pages: RefCell<HashMap<PathBuf, ResDrive>>,
        pub network_pages: RefCell<HashMap<PathBuf, ResNetwork>>,

        pub settings: gio::Settings,
    }

    impl Default for MainWindow {
        fn default() -> Self {
            Self {
                drive_pages: RefCell::default(),
                network_pages: RefCell::default(),
                flap: TemplateChild::default(),
                resources_sidebar: TemplateChild::default(),
                content_stack: TemplateChild::default(),
                applications: TemplateChild::default(),
                applications_page: TemplateChild::default(),
                processes: TemplateChild::default(),
                processes_page: TemplateChild::default(),
                cpu: TemplateChild::default(),
                cpu_page: TemplateChild::default(),
                memory: TemplateChild::default(),
                memory_page: TemplateChild::default(),
                settings: gio::Settings::new(APP_ID),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MainWindow {
        const NAME: &'static str = "MainWindow";
        type Type = super::MainWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MainWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }

            // Load latest window state
            obj.load_window_size();
        }
    }

    impl WidgetImpl for MainWindow {}

    impl WindowImpl for MainWindow {
        // Save window state on delete event
        fn close_request(&self) -> gtk::Inhibit {
            if let Err(err) = self.obj().save_window_size() {
                log::warn!("Failed to save window state, {}", &err);
            }

            // Pass close request on to the parent
            self.parent_close_request()
        }
    }

    impl ApplicationWindowImpl for MainWindow {}

    impl AdwApplicationWindowImpl for MainWindow {}
}

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup, gtk::Root;
}

impl MainWindow {
    pub fn new(app: &Application) -> Self {
        let window = glib::Object::builder::<Self>()
            .property("application", app)
            .build();

        window.setup_widgets();
        window
    }

    fn setup_widgets(&self) {
        let imp = self.imp();
        imp.applications.init();
        imp.processes.init();
        imp.cpu.init();
        imp.memory.init();

        let main_context = MainContext::default();
        main_context.spawn_local(clone!(@strong self as this => async move {
            let imp = this.imp();

            let gpus = GPU::get_gpus().await.unwrap_or_default();
            let mut i = 1;
            for gpu in &gpus {
                let page = ResGPU::new();
                page.init(gpu.clone(), i);
                if gpus.len() > 1 {
                    imp.content_stack
                        .add_titled(&page, None, &i18n_f("GPU {}", &[&i.to_string()]));
                    i += 1;
                } else {
                    imp.content_stack
                        .add_titled(&page, None, &i18n("GPU"));
                    i += 1;
                }
            }

            futures_util::join!(
            async {
                loop {
                    this.watch_for_drives().await;
                    timeout_future_seconds(1).await;
                }
            }, async {
                loop {
                    this.watch_for_network_interfaces().await;
                    timeout_future_seconds(1).await;
                }
            });
        }));
    }

    async fn watch_for_drives(&self) {
        let imp = self.imp();
        let mut still_active_drives = Vec::with_capacity(imp.drive_pages.borrow().len());
        for path in Drive::get_sysfs_paths(true).await.unwrap_or_default() {
            // ignore drive pages that are already listed
            if imp.drive_pages.borrow().contains_key(&path) {
                still_active_drives.push(path);
                continue;
            }
            if let Ok(drive) = Drive::from_sysfs(&path).await {
                let (capacity_trunc, prefix) = to_largest_unit(
                    (drive.capacity().await.unwrap_or(0) * drive.sector_size().await.unwrap_or(512))
                        as f64,
                    &Base::Decimal,
                );
                let title = match drive.drive_type {
                    DriveType::CdDvdBluray => i18n("CD/DVD/Blu-ray Drive"),
                    DriveType::Floppy => i18n("Floppy Drive"),
                    _ => i18n_f(
                        "{} {}B Drive",
                        &[&capacity_trunc.round().to_string(), prefix],
                    ),
                };

                let page = ResDrive::new();
                page.init(drive, title.clone());
                imp.content_stack.add_titled(&page, None, &title);
                imp.drive_pages.borrow_mut().insert(path.clone(), page);
                still_active_drives.push(path);
            }
        }
        // remove all the pages of drives that have been removed from the system
        // during the last time this method was called and now
        imp.drive_pages
            .borrow_mut()
            .drain_filter(|k, _| !still_active_drives.iter().any(|x| *x == *k)) // remove entry from drives HashMap
            .for_each(|(_, page)| {
                imp.content_stack.remove(&page);
            }); // remove page from the UI
    }

    async fn watch_for_network_interfaces(&self) {
        let imp = self.imp();
        let mut still_active_interfaces = Vec::with_capacity(imp.network_pages.borrow().len());
        for path in NetworkInterface::get_sysfs_paths()
            .await
            .unwrap_or_default()
        {
            // ignore network pages that are already listed
            if imp.network_pages.borrow().contains_key(&path) {
                still_active_interfaces.push(path);
                continue;
            }
            if let Ok(interface) = NetworkInterface::from_sysfs(&path).await {
                let sidebar_title = match interface.interface_type {
                    InterfaceType::Ethernet => i18n("Ethernet Connection"),
                    InterfaceType::InfiniBand => i18n("InfiniBand Connection"),
                    InterfaceType::Slip => i18n("Serial Line IP Connection"),
                    InterfaceType::Wlan => i18n("Wi-Fi Connection"),
                    InterfaceType::Wwan => i18n("WWAN Connection"),
                    InterfaceType::Bluetooth => i18n("Bluetooth Tether"),
                    InterfaceType::Wireguard => i18n("VPN Tunnel (WireGuard)"),
                    InterfaceType::Other => i18n("Network Interface"),
                };
                let page = ResNetwork::new();
                page.init(interface);
                imp.content_stack.add_titled(&page, None, &sidebar_title);
                imp.network_pages.borrow_mut().insert(path.clone(), page);
                still_active_interfaces.push(path);
            }
        }
        // remove all the pages of network interfaces that have been removed from the system
        // during the last time this method was called and now
        imp.network_pages
            .borrow_mut()
            .drain_filter(|k, _| !still_active_interfaces.iter().any(|x| *x == *k)) // remove entry from network_pages HashMap
            .for_each(|(_, v)| imp.content_stack.remove(&v)); // remove page from the UI
    }

    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let imp = self.imp();

        let (width, height) = self.default_size();

        imp.settings.set_int("window-width", width)?;
        imp.settings.set_int("window-height", height)?;

        imp.settings
            .set_boolean("is-maximized", self.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let imp = self.imp();

        let width = imp.settings.int("window-width");
        let height = imp.settings.int("window-height");
        let is_maximized = imp.settings.boolean("is-maximized");

        self.set_default_size(width, height);

        if is_maximized {
            self.maximize();
        }
    }
}

impl Default for MainWindow {
    fn default() -> Self {
        Application::default()
            .active_window()
            .unwrap()
            .downcast()
            .unwrap()
    }
}
