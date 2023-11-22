use hashbrown::HashMap;
use process_data::ProcessData;
use std::path::PathBuf;
use std::time::Duration;

use adw::{prelude::*, subclass::prelude::*};
use adw::{Toast, ToastOverlay};
use anyhow::Result;
use gtk::glib::{clone, timeout_future, MainContext};
use gtk::{gio, glib, Widget};

use crate::application::Application;
use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f, ni18n_f};
use crate::ui::pages::applications::ResApplications;
use crate::ui::pages::drive::ResDrive;
use crate::ui::pages::processes::ResProcesses;
use crate::utils::app::AppsContext;
use crate::utils::cpu::CpuData;
use crate::utils::drive::{Drive, DriveData};
use crate::utils::gpu::{Gpu, GpuData};
use crate::utils::memory::MemoryData;
use crate::utils::network::{NetworkData, NetworkInterface};
use crate::utils::process::{Process, ProcessAction};
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

    use gtk::{
        glib::{Receiver, Sender},
        CompositeTemplate,
    };
    use process_data::pci_slot::PciSlot;

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

        pub drive_pages: RefCell<HashMap<PathBuf, adw::ToolbarView>>,

        pub network_pages: RefCell<HashMap<PathBuf, adw::ToolbarView>>,

        pub gpu_pages: RefCell<HashMap<PciSlot, (Gpu, adw::ToolbarView)>>,

        pub apps_context: RefCell<AppsContext>,

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

struct RefreshData {
    cpu_data: CpuData,
    mem_data: MemoryData,
    gpu_data: Vec<GpuData>,
    drive_paths: Vec<PathBuf>,
    drive_data: Vec<DriveData>,
    network_paths: Vec<PathBuf>,
    network_data: Vec<NetworkData>,
    process_data: Vec<ProcessData>,
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

    fn init_gpu_pages(self: &MainWindow) -> Vec<Gpu> {
        let imp = self.imp();

        let gpus = Gpu::get_gpus().unwrap_or_default();
        let gpus_len = gpus.len();

        for (i, gpu) in gpus.iter().enumerate() {
            let page = ResGPU::new();

            let title = if gpus_len > 1 {
                i18n_f("GPU {}", &[&i.to_string()])
            } else {
                i18n("GPU")
            };

            page.set_tab_name(&*title);

            let added_page = if let Ok(gpu_name) = gpu.name() {
                self.add_page(&page, &title, &gpu_name)
            } else {
                self.add_page(&page, &title, &title)
            };

            page.init(gpu, i);

            imp.gpu_pages
                .borrow_mut()
                .insert(gpu.pci_slot().clone(), (gpu.clone(), added_page));
        }
        gpus
    }

    fn setup_widgets(&self) {
        let imp = self.imp();

        imp.resources_sidebar.set_stack(&imp.content_stack);

        imp.applications.init(imp.sender.clone());
        imp.processes.init(imp.sender.clone());
        imp.memory.init();

        if SETTINGS.show_search_on_start() {
            imp.processes.toggle_search();
            imp.applications.toggle_search();
        }

        *self.imp().apps_context.borrow_mut() = AppsContext::new();

        self.imp().cpu.init();

        self.init_gpu_pages();

        let main_context = MainContext::default();
        main_context.spawn_local(clone!(@strong self as this => async move {
            this.periodic_refresh_all().await;
        }));
    }

    fn gather_refresh_data(logical_cpus: usize, gpus: &[Gpu]) -> RefreshData {
        let cpu_data = CpuData::new(logical_cpus);

        let mem_data = MemoryData::new();

        let mut gpu_data = vec![];
        for gpu in gpus {
            let data = GpuData::new(gpu);

            gpu_data.push(data);
        }

        let mut drive_data = vec![];
        let drive_paths = Drive::get_sysfs_paths().unwrap();
        for path in &drive_paths {
            let data = DriveData::new(path);

            drive_data.push(data);
        }

        let mut network_data = vec![];
        let network_paths = NetworkInterface::get_sysfs_paths().unwrap();
        for path in &network_paths {
            let data = NetworkData::new(path);

            network_data.push(data);
        }

        let process_data = Process::all_data().unwrap();

        RefreshData {
            cpu_data,
            mem_data,
            gpu_data,
            drive_paths,
            drive_data,
            network_paths,
            network_data,
            process_data,
        }
    }

    fn refresh_ui(&self, refresh_data: RefreshData) {
        let imp = self.imp();

        let RefreshData {
            cpu_data,
            mem_data,
            gpu_data,
            drive_paths,
            drive_data,
            network_paths,
            network_data,
            process_data,
        } = refresh_data;

        /*
         * Apps and processes
         */

        let mut apps_context = imp.apps_context.borrow_mut();
        apps_context.refresh(process_data);

        imp.applications.refresh_apps_list(&apps_context);
        imp.processes.refresh_processes_list(&apps_context);

        /*
         *  Gpu
         */
        let gpu_pages = imp.gpu_pages.borrow();
        for ((_, page), mut gpu_data) in gpu_pages.values().zip(gpu_data) {
            let page = page.content().and_downcast::<ResGPU>().unwrap();

            // non-NVIDIA GPUs unfortunately don't expose encoder/decoder stats centrally but
            // rather expose them only per-process. since we've just refreshed our
            // processes, we take the opportunity and collect the encoder/decoder for the
            // current GPU and slip it into GpuData just in time

            if gpu_data.usage_fraction.is_none() {
                gpu_data.usage_fraction = Some(apps_context.gpu_fraction(gpu_data.pci_slot) as f64);
            }

            if gpu_data.encode_fraction.is_none() {
                gpu_data.encode_fraction =
                    Some(apps_context.encoder_fraction(gpu_data.pci_slot) as f64);
            }

            if gpu_data.decode_fraction.is_none() {
                gpu_data.decode_fraction =
                    Some(apps_context.decoder_fraction(gpu_data.pci_slot) as f64);
            }

            page.refresh_page(gpu_data);
        }

        std::mem::drop(apps_context);

        /*
         * Cpu
         */
        imp.cpu.refresh_page(&cpu_data);

        /*
         * Memory
         */
        imp.memory.refresh_page(mem_data);

        /*
         *  Drives
         */
        // Make sure there is a page for every drive that is shown
        self.refresh_drive_pages(drive_paths, &drive_data);

        // Update drive pages
        for drive_data in drive_data.into_iter() {
            if drive_data.is_virtual && !SETTINGS.show_virtual_drives() {
                continue;
            }

            let drive_pages = imp.drive_pages.borrow();
            let page = drive_pages.get(&drive_data.inner.sysfs_path).unwrap();
            let page = page.content().and_downcast::<ResDrive>().unwrap();

            page.refresh_page(drive_data);
        }

        /*
         *  Network
         */
        // Make sure there is a page for every network interface that is shown
        self.refresh_network_pages(network_paths, &network_data);

        // Update network pages
        for network_data in network_data.into_iter() {
            if network_data.is_virtual && !SETTINGS.show_virtual_network_interfaces() {
                continue;
            }

            let network_pages = imp.network_pages.borrow();
            let page = network_pages.get(&network_data.inner.sysfs_path).unwrap();
            let page = page.content().and_downcast::<ResNetwork>().unwrap();

            page.refresh_page(network_data);
        }
    }

    pub async fn periodic_refresh_all(&self) {
        let imp = self.imp();
        let gpus = Gpu::get_gpus().unwrap_or_default();
        let logical_cpus = imp.cpu.imp().logical_cpus_amount.get();

        let (tx_data, rx_data) = std::sync::mpsc::sync_channel(1);
        let (tx_wait, rx_wait) = std::sync::mpsc::sync_channel(1);

        std::thread::spawn(move || {
            loop {
                let data = Self::gather_refresh_data(logical_cpus, &gpus);
                tx_data.send(data).unwrap();

                // Wait on delay so we don't gather data multiple times in a short time span
                // Which usually just yields the same data and makes changes appear delayed by (up to) multiple refreshes
                let _wait = rx_wait.recv().unwrap();
            }
        });

        loop {
            // gather_refresh_data()
            let refresh_data = rx_data.recv().unwrap();

            self.refresh_ui(refresh_data);

            // Total time before next ui refresh
            let total_delay = SETTINGS.refresh_speed().ui_refresh_interval();

            // Reasonable timespan before total_delay ends to gather all data
            let gather_time = 0.2;

            timeout_future(Duration::from_secs_f32(total_delay - gather_time)).await;

            // Tell other threads to start gethering data
            tx_wait.send(()).unwrap();

            timeout_future(Duration::from_secs_f32(gather_time)).await;
        }
    }

    /// Create page for every drive that is shown
    fn refresh_drive_pages(&self, mut paths: Vec<PathBuf>, drive_data: &[DriveData]) {
        let imp = self.imp();

        let mut drive_pages = imp.drive_pages.borrow_mut();

        let old_page_paths: Vec<PathBuf> = drive_pages
            .iter()
            .map(|(path, _)| path.to_owned())
            .collect();

        // Filter hidden drives
        for data in drive_data {
            if data.is_virtual && !SETTINGS.show_virtual_drives() {
                let idx = paths
                    .iter()
                    .position(|p| **p == data.inner.sysfs_path)
                    .unwrap();
                paths.remove(idx);
            }
        } // paths now contains all the (paths to) drives we want to show

        // Delete hidden old drive pages
        for page_path in &old_page_paths {
            if !paths.contains(page_path) {
                // A drive has been removed

                let page = drive_pages.remove(page_path).unwrap();
                imp.content_stack.remove(&page);
            }
        }

        // Add new drive pages
        for path in paths {
            if !drive_pages.contains_key(&path) {
                // A drive has been added

                let drive = drive_data
                    .iter()
                    .find(|d| d.inner.sysfs_path == path)
                    .unwrap();

                let display_name = drive.inner.display_name(drive.capacity as f64);

                let page = ResDrive::new();
                page.init(drive);

                let toolbar = if let Some(model) = &drive.inner.model {
                    self.add_page(&page, model, &display_name)
                } else {
                    self.add_page(&page, &display_name, "")
                };

                drive_pages.insert(path, toolbar);
            }
        }
    }

    /// Create page for every network interface that is shown
    fn refresh_network_pages(&self, mut paths: Vec<PathBuf>, network_data: &[NetworkData]) {
        let imp = self.imp();

        let mut network_pages = imp.network_pages.borrow_mut();

        let old_page_paths: Vec<PathBuf> = network_pages
            .iter()
            .map(|(path, _)| path.to_owned())
            .collect();

        // Filter hidden networks
        for data in network_data {
            if data.is_virtual && !SETTINGS.show_virtual_network_interfaces() {
                let idx = paths
                    .iter()
                    .position(|p| **p == data.inner.sysfs_path)
                    .unwrap();
                paths.remove(idx);
            }
        } // paths now contains all the (paths to) network interfaces we want to show

        // Delete hidden old network pages
        for page_path in &old_page_paths {
            if !paths.contains(page_path) {
                // A network interface has been removed

                let page = network_pages.remove(page_path).unwrap();
                imp.content_stack.remove(&page);
            }
        }

        // Add new network pages
        for path in paths {
            if !network_pages.contains_key(&path) {
                // A network interface has been added

                let network_interface = network_data
                    .iter()
                    .find(|d| d.inner.sysfs_path == path)
                    .unwrap();

                // Insert stub page, values will be updated in refresh_page()
                let page = ResNetwork::new();
                page.init(network_interface);

                let toolbar = self.add_page(
                    &page,
                    &network_interface.inner.display_name(),
                    &network_interface.inner.interface_type.to_string(),
                );

                network_pages.insert(path.clone(), toolbar);
            }
        }
    }

    fn process_action(&self, action: Action) -> glib::ControlFlow {
        let main_context = MainContext::default();
        main_context.spawn_local(clone!(@strong self as this => async move {
            let imp = this.imp();
            let apps_context = imp.apps_context.borrow();
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

        imp.content_stack.add_child(&toolbar);

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
