use std::collections::HashMap;
use std::path::PathBuf;

use adw::{prelude::*, subclass::prelude::*};
use adw::{Toast, ToastOverlay};
use anyhow::Result;
use gtk::glib::{clone, timeout_future_seconds, MainContext};
use gtk::{gio, glib, Widget};

use crate::application::Application;
use crate::config::{APP_ID, PROFILE};
use crate::i18n::{i18n, i18n_f};
use crate::ui::pages::drive::ResDrive;
use crate::utils::drive::{Drive, DriveType};
use crate::utils::gpu::GPU;
use crate::utils::network::{InterfaceType, NetworkInterface};
use crate::utils::processes::{AppsContext, ProcessAction};
use crate::utils::units::{to_largest_unit, Base};

use super::pages::gpu::ResGPU;
use super::pages::network::ResNetwork;

#[derive(Debug, Clone)]
pub enum Action {
    ManipulateProcess(ProcessAction, i32, String, ToastOverlay),
    ManipulateApp(ProcessAction, String, ToastOverlay),
}

mod imp {
    use std::cell::RefCell;

    use crate::{
        ui::pages::{
            applications::ResApplications, cpu::ResCPU, memory::ResMemory, processes::ResProcesses,
        },
        utils::processes::AppsContext,
    };

    use super::*;

    use gtk::{
        glib::{Receiver, Sender},
        CompositeTemplate,
    };

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/me/nalux/Resources/ui/window.ui")]
    pub struct MainWindow {
        #[template_child]
        pub split_view: TemplateChild<adw::OverlaySplitView>,
        #[template_child]
        pub processor_window_title: TemplateChild<adw::WindowTitle>,
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

        pub drive_pages: RefCell<HashMap<PathBuf, adw::ToolbarView>>,

        pub network_pages: RefCell<HashMap<PathBuf, adw::ToolbarView>>,

        pub apps_context: RefCell<AppsContext>,

        pub sender: Sender<Action>,
        pub receiver: RefCell<Option<Receiver<Action>>>,

        pub settings: gio::Settings,
    }

    impl Default for MainWindow {
        fn default() -> Self {
            let (sender, r) = glib::MainContext::channel(glib::Priority::default());
            let receiver = RefCell::new(Some(r));

            Self {
                drive_pages: RefCell::default(),
                network_pages: RefCell::default(),
                split_view: TemplateChild::default(),
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
                apps_context: RefCell::default(),
                settings: gio::Settings::new(APP_ID),
                sender,
                receiver,
                processor_window_title: TemplateChild::default(),
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
        fn close_request(&self) -> glib::Propagation {
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

        let imp = window.imp();

        imp.receiver.borrow_mut().take().unwrap().attach(
            None,
            clone!(@strong window => move |action| window.process_action(action)),
        );

        window.setup_widgets();
        window
    }

    fn setup_widgets(&self) {
        let imp = self.imp();

        imp.applications.init(imp.sender.clone());
        imp.processes.init(imp.sender.clone());
        imp.cpu.init(&imp.processor_window_title);
        imp.memory.init();

        let main_context = MainContext::default();
        main_context.spawn_local(clone!(@strong self as this => async move {
            let imp = this.imp();

            *imp.apps_context.borrow_mut() = AppsContext::new().await.unwrap();

            let gpus = GPU::get_gpus().await.unwrap_or_default();
            for (i, gpu) in gpus.iter().enumerate() {
                let page = ResGPU::new();
                page.init(gpu.clone(), i);
                let title = if gpus.len() > 1 {
                    i18n_f("GPU {}", &[&i.to_string()])
                } else {
                    i18n("GPU")
                };
                if let Ok(gpu_name) = gpu.get_name() {
                    this.add_page(&page, &title, &gpu_name, &title);
                } else {
                    this.add_page(&page, &title, &title, "");
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
            }, async {
                loop {
                    let _ = imp.apps_context.borrow_mut().refresh().await;
                    imp.applications.refresh_apps_list(&imp.apps_context.borrow());
                    imp.processes.refresh_processes_list(&imp.apps_context.borrow());
                    timeout_future_seconds(2).await;
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
                page.init(drive.clone());
                /*imp.content_stack.add_titled(&page, None, &title);*/
                let toolbar = if let Some(model) = drive.model {
                    self.add_page(&page, &title, &model, &title)
                } else {
                    self.add_page(&page, &title, &title, "")
                };
                imp.drive_pages.borrow_mut().insert(path.clone(), toolbar);
                still_active_drives.push(path);
            }
        }
        // remove all the pages of drives that have been removed from the system
        // during the last time this method was called and now
        imp.drive_pages
            .borrow_mut()
            .extract_if(|k, _| !still_active_drives.iter().any(|x| *x == *k)) // remove entry from drives HashMap
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
                page.init(interface.clone());
                //imp.content_stack.add_titled(&page, None, &sidebar_title);
                let toolbar = self.add_page(
                    &page,
                    &sidebar_title,
                    &interface.display_name(),
                    &sidebar_title,
                );
                imp.network_pages.borrow_mut().insert(path.clone(), toolbar);
                still_active_interfaces.push(path);
            }
        }
        // remove all the pages of network interfaces that have been removed from the system
        // during the last time this method was called and now
        imp.network_pages
            .borrow_mut()
            .extract_if(|k, _| !still_active_interfaces.iter().any(|x| *x == *k)) // remove entry from network_pages HashMap
            .for_each(|(_, v)| imp.content_stack.remove(&v)); // remove page from the UI
    }

    fn process_action(&self, action: Action) -> glib::ControlFlow {
        let imp = self.imp();

        match action {
            Action::ManipulateProcess(action, pid, display_name, toast_overlay) => {
                let apps_context = imp.apps_context.borrow();
                if let Some(process) = apps_context.get_process(pid) {
                    let toast_message = match process.execute_process_action(action) {
                        Ok(_) => get_action_success(action, &[&display_name]),
                        Err(e) => {
                            log::error!("Unable to kill process {}: {}", pid, e);
                            get_process_action_failure(action, &[&display_name])
                        }
                    };
                    toast_overlay.add_toast(Toast::new(&toast_message));
                }
            }

            Action::ManipulateApp(action, id, toast_overlay) => {
                let apps = imp.apps_context.borrow();
                let app = apps.get_app(&id).unwrap();
                let res = app.execute_process_action(&apps, action);

                for r in res.iter() {
                    if let Err(e) = r {
                        log::error!("Unable to kill a process: {}", e);
                    }
                }

                let processes_tried = res.len();
                let processes_successful = res.iter().flatten().count();
                let processes_unsuccessful = processes_tried - processes_successful;

                #[rustfmt::skip]
                let toast_message = match processes_unsuccessful {
                    0 => get_action_success(action, &[&app.display_name]),
                    1 => get_app_action_failure(action),
                    _ => get_app_action_failure_multiple(action, &[&processes_unsuccessful.to_string()]),
                };

                toast_overlay.add_toast(Toast::new(&toast_message));
            }
        };

        glib::ControlFlow::Continue
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

    fn add_page(
        &self,
        widget: &impl IsA<Widget>,
        sidebar_title: &str,
        window_title: &str,
        window_subtitle: &str,
    ) -> adw::ToolbarView {
        let imp = self.imp();

        let title_widget = adw::WindowTitle::new(window_title, window_subtitle);

        let sidebar_button = gtk::ToggleButton::new();
        sidebar_button.set_icon_name("sidebar-show-symbolic");
        imp.split_view
            .bind_property("collapsed", &sidebar_button, "visible")
            .sync_create()
            .build();
        imp.split_view
            .bind_property("show-sidebar", &sidebar_button, "active")
            .sync_create()
            .bidirectional()
            .build();

        let header_bar = adw::HeaderBar::new();
        header_bar.add_css_class("flat");
        header_bar.set_title_widget(Some(&title_widget));
        header_bar.pack_start(&sidebar_button);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header_bar);
        toolbar.set_content(Some(widget));

        imp.content_stack.add_titled(&toolbar, None, sidebar_title);

        toolbar
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

pub fn get_action_name(action: ProcessAction, args: &[&str]) -> String {
    match action {
        ProcessAction::TERM => i18n_f("End {}?", args),
        ProcessAction::STOP => i18n_f("Halt {}?", args),
        ProcessAction::KILL => i18n_f("Kill {}?", args),
        ProcessAction::CONT => i18n_f("Continue {}?", args),
    }
}

pub fn get_app_action_warning(action: ProcessAction) -> String {
    match action {
            ProcessAction::TERM => i18n("Unsaved work might be lost."),
            ProcessAction::STOP => i18n("Halting an application can come with serious risks such as losing data and security implications. Use with caution."),
            ProcessAction::KILL => i18n("Killing an application can come with serious risks such as losing data and security implications. Use with caution."),
            ProcessAction::CONT => "".into(),
        }
}

pub fn get_app_action_description(action: ProcessAction) -> String {
    match action {
        ProcessAction::TERM => i18n("End application"),
        ProcessAction::STOP => i18n("Halt application"),
        ProcessAction::KILL => i18n("Kill application"),
        ProcessAction::CONT => i18n("Continue application"),
    }
}

pub fn get_action_success(action: ProcessAction, args: &[&str]) -> String {
    match action {
        ProcessAction::TERM => i18n_f("Successfully ended {}", args),
        ProcessAction::STOP => i18n_f("Successfully halted {}", args),
        ProcessAction::KILL => i18n_f("Successfully killed {}", args),
        ProcessAction::CONT => i18n_f("Successfully continued {}", args),
    }
}

pub fn get_app_action_failure(action: ProcessAction) -> String {
    match action {
        ProcessAction::TERM => i18n("There was a problem ending a process"),
        ProcessAction::STOP => i18n("There was a problem halting a process"),
        ProcessAction::KILL => i18n("There was a problem killing a process"),
        ProcessAction::CONT => i18n("There was a problem continuing a process"),
    }
}

pub fn get_process_action_failure(action: ProcessAction, args: &[&str]) -> String {
    match action {
        ProcessAction::TERM => i18n_f("There was a problem ending {}", args),
        ProcessAction::STOP => i18n_f("There was a problem halting {}", args),
        ProcessAction::KILL => i18n_f("There was a problem killing {}", args),
        ProcessAction::CONT => i18n_f("There was a problem continuing {}", args),
    }
}

pub fn get_app_action_failure_multiple(action: ProcessAction, args: &[&str]) -> String {
    match action {
        ProcessAction::TERM => i18n_f("There were problems ending {} processes", args),
        ProcessAction::STOP => i18n_f("There were problems halting {} processes", args),
        ProcessAction::KILL => i18n_f("There were problems killing {} processes", args),
        ProcessAction::CONT => i18n_f("There were problems continuing {} processes", args),
    }
}
