use std::collections::HashMap;
use std::path::PathBuf;

use adw::{prelude::*, subclass::prelude::*};
use anyhow::{Context, Result};
use futures_util::stream::StreamExt;
use gtk::glib::{clone, timeout_future_seconds, MainContext};
use gtk::{gio, glib};
use zbus::export::futures_util;
use zbus::Connection;
use zvariant::Value::{Array, Bool, ObjectPath, U8};

use crate::application::Application;
use crate::config::{APP_ID, PROFILE};
use crate::dbus_proxies::udisks2::{
    BlockProxy, DriveProxy, InterfacesAdded, PartitionProxy, SwapspaceProxy,
    UDisks2InterfacesProxy, UDisks2ManagerProxy,
};
use crate::ui::pages::drive::ResDrive;
use crate::ui::pages::network::ResNetwork;
use crate::utils::gpu::GPU;
use crate::utils::network::InterfaceType;
use crate::utils::network::NetworkInterface;
use crate::utils::units::{to_largest_unit, Base};

use super::pages::gpu::ResGPU;

mod imp {
    use std::cell::RefCell;

    use crate::ui::pages::{cpu::ResCPU, memory::ResMemory, network::ResNetwork};

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/me/nalux/Resources/ui/window.ui")]
    pub struct MainWindow {
        pub drive_pages: RefCell<HashMap<String, ResDrive>>,
        pub network_pages: RefCell<HashMap<PathBuf, ResNetwork>>,
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
        pub memory: TemplateChild<ResMemory>,
        #[template_child]
        pub memory_page: TemplateChild<gtk::StackPage>,

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
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

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
        fn close_request(&self, window: &Self::Type) -> gtk::Inhibit {
            if let Err(err) = window.save_window_size() {
                log::warn!("Failed to save window state, {}", &err);
            }

            // Pass close request on to the parent
            self.parent_close_request(window)
        }
    }

    impl ApplicationWindowImpl for MainWindow {}

    impl AdwApplicationWindowImpl for MainWindow {}
}

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup, gtk::Root;
}

impl MainWindow {
    pub fn new(app: &Application) -> Self {
        let window = glib::Object::new::<Self>(&[("application", app)])
            .expect("Failed to create MainWindow");
        window.setup_widgets();
        window
    }

    fn setup_widgets(&self) {
        let imp = self.imp();
        imp.cpu.init();
        imp.memory.init();
        let gpus = GPU::get_gpus().unwrap_or_default();
        let mut i = 1;
        for gpu in &gpus {
            let page = ResGPU::new();
            page.init(gpu.clone(), i);
            if gpus.len() > 1 {
                imp.content_stack
                    .add_titled(&page, None, &gettextrs::gettext!("GPU {}", i));
                i += 1;
            } else {
                imp.content_stack
                    .add_titled(&page, None, &gettextrs::gettext("GPU"));
                i += 1;
            }
        }

        let main_context = MainContext::default();
        main_context.spawn_local(clone!(@strong self as this => async move {
            this.look_for_drives().await.unwrap_or_default();
            futures_util::try_join!(
                this.watch_for_drives(),
                async {
                    // because NetworkManager exposes weird "virtual" devices,
                    // is inconsistent (at least for our case) with its UDI
                    // path, we watch for network interfaces the old-fashioned
                    // way: just poll /sys/class/net/ every second
                    loop {
                        this.watch_for_network_interfaces()?;
                        timeout_future_seconds(1).await;
                    }
                    #[allow(unreachable_code)]
                    Ok(())    // this is to make the compiler happy
                }
            ).unwrap_or_default();
        }));
    }

    async fn look_for_drives(&self) -> Result<()> {
        let imp = self.imp();
        let conn = Connection::system()
            .await
            .with_context(|| "unable to establish connection to system bus")?;
        let manager = UDisks2ManagerProxy::new(&conn)
            .await
            .with_context(|| "unable to connect to UDisks2 bus")?;
        let block_devices = manager
            .get_block_devices(HashMap::new())
            .await
            .with_context(|| "unable to get connected devices")?;
        for block_device in &block_devices {
            let block = BlockProxy::builder(&conn)
                .path(block_device)?
                .build()
                .await?;
            // This is an incredibly awkward way to make sure that this block device is neither
            // a partition nor a swapspace: try to get a property from the UDisks2 Partition
            // (or Swapspace) dbus interface and proceed if it fails
            // TODO: make this less horrible
            let is_partition = PartitionProxy::builder(&conn)
                .path(block_device)?
                .build()
                .await?
                .name()
                .await
                .is_ok();
            let is_swapspace = SwapspaceProxy::builder(&conn)
                .path(block_device)?
                .build()
                .await?
                .active()
                .await
                .is_ok();
            let has_crypto_backing_device = block.crypto_backing_device().await?.as_str() != "/";
            let drive_object_path = block.drive().await?;
            if !is_partition && !is_swapspace && !has_crypto_backing_device {
                if let Ok(drive) = DriveProxy::builder(&conn)
                    .path(&drive_object_path)?
                    .build()
                    .await
                {
                    let drive_page = ResDrive::new();
                    let vendor = drive.vendor().await?;
                    let model = drive.model().await?;
                    let mut device = String::new();
                    if let Ok(device_bytes) = block.device().await {
                        device = String::from_utf8(
                            device_bytes.into_iter().filter(|x| *x != 0).collect(),
                        )?;
                    }
                    let capacity = drive.size().await?;
                    let formatted_capacity = to_largest_unit(capacity as f64, Base::Decimal);
                    let capacity_string = format!(
                        "{} {}B",
                        formatted_capacity.0.round() as f64,
                        formatted_capacity.1
                    );
                    let mut writable = true;
                    if let Ok(ro) = block.read_only().await {
                        writable = !ro;
                    }
                    let removable = drive.removable().await?;
                    drive_page.init(&vendor, &model, &device, capacity, writable, removable);
                    imp.content_stack.add_titled(
                        &drive_page,
                        None,
                        &gettextrs::gettext!("{} Drive", capacity_string),
                    );
                    imp.drive_pages
                        .borrow_mut()
                        .insert(drive_object_path.to_string(), drive_page);
                }
            }
        }
        Ok(())
    }

    async fn watch_for_drives(&self) -> Result<()> {
        let imp = self.imp();
        let conn = Connection::system()
            .await
            .with_context(|| "unable to establish connection to system bus")?;
        let manager = UDisks2InterfacesProxy::new(&conn)
            .await
            .with_context(|| "unable to connect to UDisks2 bus")?;
        let mut interfaces_added = manager
            .receive_interfaces_added()
            .await
            .with_context(|| "unable to establish connection to UDisk2's InterfacesAdded")?;
        let mut interfaces_removed = manager
            .receive_interfaces_removed()
            .await
            .with_context(|| "unable to establish connection to UDisk2's InterfacesRemoved")?;
        futures_util::try_join!(
            async {
                while let Some(signal) = interfaces_added.next().await {
                    if let Some(result) = Self::handle_income_signals(signal, &conn).await? {
                        let capacity = to_largest_unit(result.2 as f64, Base::Decimal);
                        let capacity_string =
                            format!("{} {}B", capacity.0.round() as usize, capacity.1);
                        imp.content_stack.add_titled(
                            &result.1,
                            None,
                            &gettextrs::gettext!("{} Drive", capacity_string),
                        );
                        imp.drive_pages.borrow_mut().insert(result.0, result.1);
                    }
                }
                Ok::<(), anyhow::Error>(())
            },
            async {
                while let Some(signal) = interfaces_removed.next().await {
                    let body: (zbus::zvariant::ObjectPath, Vec<String>) = signal.body()?;
                    if body.1.iter().any(|x| x == "org.freedesktop.UDisks2.Drive") {
                        let mut borrowed_drive_pages = imp.drive_pages.borrow_mut();
                        if let Some(drive_page) = borrowed_drive_pages.get(body.0.as_str()) {
                            imp.content_stack.remove(drive_page);
                            borrowed_drive_pages.remove(body.0.as_str());
                        }
                    }
                }
                Ok(())
            }
        )
        .map(|_| ())
        .with_context(|| "async drive watchers failed")
    }

    async fn handle_income_signals(
        signal: InterfacesAdded,
        conn: &Connection,
    ) -> Result<Option<(String, ResDrive, u64)>> {
        let body: (
            zbus::zvariant::ObjectPath,
            HashMap<String, HashMap<String, zbus::zvariant::Value>>,
        ) = signal.body()?;
        // we want to grab the signal containing the block device of the inserted drive, not the `Drive`
        // itself nor any of its partitions, mainly because `Drive` doesn't give us the /dev/ file that
        // we need for some diagnostics
        if body.1.get("org.freedesktop.UDisks2.Partition").is_none()
            && body.1.get("org.freedesktop.UDisks2.Swapspace").is_none()
        {
            if let Some(block_data) = body.1.get("org.freedesktop.UDisks2.Block") {
                if let Some(ObjectPath(object_path)) = block_data.get("Drive") {
                    let drive_page = ResDrive::new();
                    let drive = DriveProxy::builder(conn).path(object_path)?.build().await?;
                    let vendor = drive.vendor().await?;
                    let model = drive.model().await?;
                    let mut device = String::new();
                    if let Some(Array(device_bytes)) = block_data.get("Device") {
                        let unpacked_bytes: Vec<u8> = device_bytes
                            .iter()
                            .map(|x| if let U8(byte) = x { *byte } else { b'?' })
                            .filter(|x| *x != 0)
                            .collect();
                        device = String::from_utf8(unpacked_bytes)?;
                    }
                    let capacity = drive.size().await?;
                    let mut writable = true;
                    if let Some(Bool(ro)) = block_data.get("ReadOnly") {
                        writable = !ro;
                    }
                    let removable = drive.removable().await?;
                    drive_page.init(&vendor, &model, &device, capacity, writable, removable);

                    return Ok(Some((object_path.to_string(), drive_page, capacity)));
                }
            }
        }
        Ok(None)
    }

    fn watch_for_network_interfaces(&self) -> Result<()> {
        let imp = self.imp();
        let mut still_active_interfaces = Vec::new();
        if let Ok(paths) = std::fs::read_dir("/sys/class/net") {
            for path in paths.flatten() {
                let dir_path = path.path();
                // skip loopback (or non-UTF-8 names) and already found network pages
                if path.file_name().to_str().unwrap_or("lo") == "lo" {
                    continue;
                }
                if imp.network_pages.borrow().contains_key(&dir_path) {
                    still_active_interfaces.push(dir_path);
                    continue;
                }
                let page = ResNetwork::new();
                if let Ok(interface) = NetworkInterface::from_sysfs(dir_path.clone()) {
                    let sidebar_title = match interface.interface_type {
                        InterfaceType::Ethernet => gettextrs::gettext("Ethernet Connection"),
                        InterfaceType::InfiniBand => gettextrs::gettext("InfiniBand Connection"),
                        InterfaceType::Slip => gettextrs::gettext("Serial Line IP Connection"),
                        InterfaceType::Wlan => gettextrs::gettext("Wi-Fi Connection"),
                        InterfaceType::Wwan => gettextrs::gettext("WWAN Connection"),
                        InterfaceType::Bluetooth => gettextrs::gettext("Bluetooth Tether"),
                        InterfaceType::Other => gettextrs::gettext("Network Interface"),
                    };
                    page.init(interface);
                    imp.content_stack.add_titled(&page, None, &sidebar_title);
                    imp.network_pages.borrow_mut().insert(path.path(), page);
                    still_active_interfaces.push(dir_path);
                }
            }
        }
        // remove all the pages of network interfaces that have been removed from the system
        // during the last time this method was called and now
        imp.network_pages
            .borrow_mut()
            .drain_filter(|k, _| !still_active_interfaces.iter().any(|x| *x == **k)) // remove entry from network_pages HashMap
            .for_each(|(_, v)| imp.content_stack.remove(&v)); // remove page from the UI
        Ok(())
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
