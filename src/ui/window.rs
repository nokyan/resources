use hashbrown::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use adw::{prelude::*, subclass::prelude::*};
use adw::{Toast, ToastOverlay};
use anyhow::{Context, Result};
use gtk::glib::{clone, timeout_future, MainContext};
use gtk::{gio, glib, Widget};

use crate::application::Application;
use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f, ni18n_f};
use crate::ui::pages::applications::ResApplications;
use crate::ui::pages::drive::ResDrive;
use crate::ui::pages::processes::ResProcesses;
use crate::utils::app::AppsContext;
use crate::utils::cpu;
use crate::utils::drive::Drive;
use crate::utils::gpu::GPU;
use crate::utils::network::{InterfaceType, NetworkInterface};
use crate::utils::process::ProcessAction;
use crate::utils::settings::SETTINGS;

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
        ui::{
            pages::{
                applications::ResApplications, cpu::ResCPU, memory::ResMemory,
                processes::ResProcesses,
            },
            widgets::stack_sidebar::ResStackSidebar,
        },
        utils::app::AppsContext,
    };

    use super::*;

    use async_std::sync::Mutex;
    use gtk::{
        glib::{Receiver, Sender},
        CompositeTemplate,
    };

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/net/nokyan/Resources/ui/window.ui")]
    pub struct MainWindow {
        #[template_child]
        pub split_view: TemplateChild<adw::OverlaySplitView>,
        #[template_child]
        pub processor_window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub resources_sidebar: TemplateChild<ResStackSidebar>,
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

        pub drive_pages: RefCell<HashMap<PathBuf, (bool, adw::ToolbarView)>>,

        pub network_pages: RefCell<HashMap<PathBuf, (bool, adw::ToolbarView)>>,

        pub gpu_pages: RefCell<Vec<adw::ToolbarView>>,

        pub apps_context: Mutex<AppsContext>,

        pub sender: Sender<Action>,
        pub receiver: RefCell<Option<Receiver<Action>>>,
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
                apps_context: Default::default(),
                sender,
                receiver,
                processor_window_title: TemplateChild::default(),
                gpu_pages: RefCell::default(),
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

    pub fn toggle_search(&self) {
        let imp = self.imp();

        let selected_page = imp
            .content_stack
            .visible_child()
            .and_downcast::<adw::ToolbarView>()
            .and_then(|toolbar| toolbar.content())
            .unwrap();

        if selected_page.is::<ResApplications>() {
            imp.applications.toggle_search();
        } else if selected_page.is::<ResProcesses>() {
            imp.processes.toggle_search();
        }
    }

    fn setup_widgets(&self) {
        let imp = self.imp();

        imp.resources_sidebar.set_stack(&imp.content_stack);

        imp.applications.init(imp.sender.clone());
        imp.processes.init(imp.sender.clone());
        imp.cpu.init();
        imp.memory.init();

        if SETTINGS.show_search_on_start() {
            imp.processes.toggle_search()
        }

        if SETTINGS.show_search_on_start() {
            imp.applications.toggle_search()
        }

        let main_context = MainContext::default();
        main_context.spawn_local(clone!(@strong self as this => async move {
            let imp = this.imp();

            {
                *imp.apps_context.lock().await = AppsContext::new().await;
            }

            let cpu_info = cpu::cpu_info()
                .await
                .with_context(|| "unable to get CPUInfo")
                .unwrap_or_default();
            let imp = this.imp();

            if let Some(cpu_name) = cpu_info.model_name {
                imp.processor_window_title.set_title(&cpu_name);
                imp.processor_window_title.set_subtitle(&i18n("Processor"));
            }

            let gpus = GPU::get_gpus().await.unwrap_or_default();
            for (i, gpu) in gpus.iter().enumerate() {
                let page = ResGPU::new();
                page.init(gpu.clone(), i);

                let title = if gpus.len() > 1 {
                    i18n_f("GPU {}", &[&i.to_string()])
                } else {
                    i18n("GPU")
                };

                page.set_tab_name(&*title);

                let added_page = if let Ok(gpu_name) = gpu.get_name() {
                    this.add_page(&page, &title, &gpu_name, &title)
                } else {
                    this.add_page(&page, &title, &title, "")
                };

                imp.gpu_pages.borrow_mut().push(added_page);
            }

            let _ = this.refresh_drives().await;
            let _ = this.refresh_network_interfaces().await;

            futures_util::join!(async {
                loop {
                    let _ = imp.cpu.refresh_page().await;

                    let _ = imp.memory.refresh_page().await;

                    if let Ok(gpu_pages) = imp.gpu_pages.try_borrow() {
                        for gpu_page_toolbar in gpu_pages.iter() {
                            let _ = gpu_page_toolbar.content().and_downcast::<ResGPU>().unwrap().refresh_page().await;
                        }
                    }

                    let _ = this.refresh_drives().await;
                    if let Ok(drive_pages) = imp.drive_pages.try_borrow() {
                        for drive_page_toolbar in drive_pages.values() {
                            let _ = drive_page_toolbar.1.content().and_downcast::<ResDrive>().unwrap().refresh_page().await;
                        }
                    }

                    let _ = this.refresh_network_interfaces().await;
                    if let Ok(network_pages) = imp.network_pages.try_borrow() {
                        for network_page_toolbar in network_pages.values() {
                            let _ = network_page_toolbar.1.content().and_downcast::<ResNetwork>().unwrap().refresh_page().await;
                        }
                    }

                    timeout_future(Duration::from_secs_f32(SETTINGS.refresh_speed().ui_refresh_interval())).await;
                }
            }, async {
                loop {
                    {
                        let mut apps_context = imp.apps_context.lock().await;
                        apps_context.refresh().await;
                        imp.applications.refresh_apps_list(&apps_context);
                        imp.processes.refresh_processes_list(&apps_context);
                    }

                    timeout_future(Duration::from_secs_f32(SETTINGS.refresh_speed().process_refresh_interval())).await;
                }
            })
        }));
    }

    async fn refresh_drives(&self) -> Result<()> {
        let imp = self.imp();
        let mut still_active_drives = Vec::with_capacity(imp.drive_pages.try_borrow()?.len());
        for path in Drive::get_sysfs_paths().await.unwrap_or_default() {
            // ignore drive pages that are already listed
            if imp.drive_pages.try_borrow()?.contains_key(&path) {
                still_active_drives.push(path);
                continue;
            }
            if let Ok(drive) = Drive::from_sysfs(&path).await {
                let is_virtual = drive.is_virtual().await;
                if is_virtual && !SETTINGS.show_virtual_drives() {
                    continue;
                }

                let sidebar_title = drive.display_name().await;

                let page = ResDrive::new();
                page.init(drive.clone());
                page.set_tab_name(&*sidebar_title);

                let toolbar = if let Some(model) = drive.model {
                    self.add_page(&page, &sidebar_title, &model, &sidebar_title)
                } else {
                    self.add_page(&page, &sidebar_title, &sidebar_title, "")
                };

                imp.drive_pages
                    .try_borrow_mut()?
                    .insert(path.clone(), (is_virtual, toolbar));

                still_active_drives.push(path);
            }
        }
        // remove all the pages of drives that have been removed from the system
        // during the last time this method was called and now
        imp.drive_pages
            .try_borrow_mut()?
            .extract_if(|path, (is_virtual, _)| {
                !still_active_drives
                    .iter()
                    .any(|other_path| *other_path == *path)
                    || (!SETTINGS.show_virtual_drives() && *is_virtual)
            })
            .for_each(|(_, (_, page))| {
                imp.content_stack.remove(&page);
            }); // remove page from the UI
        Ok(())
    }

    async fn refresh_network_interfaces(&self) -> Result<()> {
        let imp = self.imp();
        let mut still_active_interfaces = Vec::with_capacity(imp.network_pages.try_borrow()?.len());
        for path in NetworkInterface::get_sysfs_paths()
            .await
            .unwrap_or_default()
        {
            // ignore network pages that are already listed
            if imp.network_pages.try_borrow()?.contains_key(&path) {
                still_active_interfaces.push(path);
                continue;
            }
            if let Ok(interface) = NetworkInterface::from_sysfs(&path).await {
                let is_virtual = interface.is_virtual();
                if is_virtual && !SETTINGS.show_virtual_network_interfaces() {
                    continue;
                }

                let sidebar_title = match interface.interface_type {
                    InterfaceType::Bluetooth => i18n("Bluetooth Tether"),
                    InterfaceType::Bridge => i18n("Network Bridge"),
                    InterfaceType::Ethernet => i18n("Ethernet Connection"),
                    InterfaceType::InfiniBand => i18n("InfiniBand Connection"),
                    InterfaceType::Slip => i18n("Serial Line IP Connection"),
                    InterfaceType::VirtualEthernet => i18n("Virtual Ethernet Device"),
                    InterfaceType::VmBridge => i18n("VM Network Bridge"),
                    InterfaceType::Wireguard => i18n("VPN Tunnel (WireGuard)"),
                    InterfaceType::Wlan => i18n("Wi-Fi Connection"),
                    InterfaceType::Wwan => i18n("WWAN Connection"),
                    InterfaceType::Unknown => i18n("Network Interface"),
                };

                let page = ResNetwork::new();
                page.init(
                    interface.clone(),
                    interface.received_bytes().await.unwrap_or(0),
                    interface.sent_bytes().await.unwrap_or(0),
                );
                page.set_tab_name(&*sidebar_title);

                let toolbar = self.add_page(
                    &page,
                    &sidebar_title,
                    &interface.display_name(),
                    &sidebar_title,
                );

                imp.network_pages
                    .try_borrow_mut()?
                    .insert(path.clone(), (is_virtual, toolbar));
                still_active_interfaces.push(path);
            }
        }
        // remove all the pages of network interfaces that have been removed from the system
        // during the last time this method was called and now
        imp.network_pages
            .try_borrow_mut()?
            .extract_if(|path, (is_virtual, _)| {
                !still_active_interfaces
                    .iter()
                    .any(|other_path| *other_path == *path)
                    || (!SETTINGS.show_virtual_network_interfaces() && *is_virtual)
            }) // remove entry from network_pages HashMap
            .for_each(|(_, (_, page))| imp.content_stack.remove(&page)); // remove page from the UI

        Ok(())
    }

    fn process_action(&self, action: Action) -> glib::ControlFlow {
        let main_context = MainContext::default();
        main_context.spawn_local(clone!(@strong self as this => async move {
            let imp = this.imp();
            let apps_context = imp.apps_context.lock().await;
            match action {
                Action::ManipulateProcess(action, pid, display_name, toast_overlay) => {
                    if let Some(process) = apps_context.get_process(pid) {
                        let toast_message = match process.execute_process_action(action) {
                            Ok(()) => get_action_success(action, &[&display_name]),
                            Err(e) => {
                                log::error!("Unable to kill process {}: {}", pid, e);
                                get_process_action_failure(action, &[&display_name])
                            }
                        };
                        toast_overlay.add_toast(Toast::new(&toast_message));
                    }
                }

                Action::ManipulateApp(action, id, toast_overlay) => {
                    let app = apps_context.get_app(&id).unwrap();
                    let res = app.execute_process_action(&apps_context, action);

                    for r in &res {
                        if let Err(e) = r {
                            log::error!("Unable to kill a process: {}", e);
                        }
                    }

                    let processes_tried = res.len();
                    let processes_successful = res.iter().flatten().count();
                    let processes_unsuccessful = processes_tried - processes_successful;

                    let toast_message = if processes_unsuccessful > 0 {
                        get_app_action_failure(action, processes_unsuccessful as u32)
                    } else {
                        get_action_success(action, &[&app.display_name])
                    };

                    toast_overlay.add_toast(Toast::new(&toast_message));
                }
            };
        }));

        glib::ControlFlow::Continue
    }

    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let (width, height) = self.default_size();

        SETTINGS.set_window_width(width)?;
        SETTINGS.set_window_height(height)?;

        SETTINGS.set_maximized(self.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let width = SETTINGS.window_width();
        let height = SETTINGS.window_height();
        let is_maximized = SETTINGS.maximized();

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
            ProcessAction::CONT => String::new(),
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

pub fn get_app_action_failure(action: ProcessAction, args: u32) -> String {
    match action {
        ProcessAction::TERM => ni18n_f(
            "There was a problem ending a process",
            "There were problems ending {} processes",
            args,
            &[&args.to_string()],
        ),
        ProcessAction::STOP => ni18n_f(
            "There was a problem halting a process",
            "There were problems halting {} processes",
            args,
            &[&args.to_string()],
        ),
        ProcessAction::KILL => ni18n_f(
            "There was a problem killing a process",
            "There were problems killing {} processes",
            args,
            &[&args.to_string()],
        ),
        ProcessAction::CONT => ni18n_f(
            "There was a problem continuing a process",
            "There were problems continuing {} processes",
            args,
            &[&args.to_string()],
        ),
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
