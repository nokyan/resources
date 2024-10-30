use std::time::{Duration, SystemTime};

use adw::{glib::property::PropertySet, prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::utils::network::{NetworkData, NetworkInterface};
use crate::utils::units::{convert_speed, convert_storage};

pub const TAB_ID_PREFIX: &str = "network";

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::{pages::NETWORK_PRIMARY_ORD, widgets::graph_box::ResGraphBox};

    use super::*;

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/network.ui")]
    #[properties(wrapper_type = super::ResNetwork)]
    pub struct ResNetwork {
        #[template_child]
        pub receiving: TemplateChild<ResGraphBox>,
        #[template_child]
        pub sending: TemplateChild<ResGraphBox>,
        #[template_child]
        pub total_received: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub total_sent: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub manufacturer: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub driver: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub interface: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub hw_address: TemplateChild<adw::ActionRow>,
        pub old_received_bytes: Cell<Option<usize>>,
        pub old_sent_bytes: Cell<Option<usize>>,
        pub last_timestamp: Cell<SystemTime>,

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get)]
        main_graph_color: glib::Bytes,

        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: RefCell<Icon>,

        #[property(get, set)]
        usage: Cell<f64>,

        #[property(get = Self::tab_name, set = Self::set_tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,

        #[property(get = Self::tab_detail_string, set = Self::set_tab_detail_string, type = glib::GString)]
        tab_detail_string: Cell<glib::GString>,

        #[property(get = Self::tab_usage_string, set = Self::set_tab_usage_string, type = glib::GString)]
        tab_usage_string: Cell<glib::GString>,

        #[property(get = Self::tab_id, set = Self::set_tab_id, type = glib::GString)]
        tab_id: Cell<glib::GString>,

        #[property(get)]
        graph_locked_max_y: Cell<bool>,

        #[property(get)]
        primary_ord: Cell<u32>,

        #[property(get, set)]
        secondary_ord: Cell<u32>,
    }

    impl ResNetwork {
        gstring_getter_setter!(tab_name, tab_detail_string, tab_usage_string, tab_id);

        pub fn icon(&self) -> Icon {
            let icon = self.icon.replace_with(|_| NetworkInterface::default_icon());
            let result = icon.clone();
            self.icon.set(icon);
            result
        }

        pub fn set_icon(&self, icon: &Icon) {
            self.icon.set(icon.clone());
        }
    }

    impl Default for ResNetwork {
        fn default() -> Self {
            Self {
                receiving: Default::default(),
                sending: Default::default(),
                total_received: Default::default(),
                total_sent: Default::default(),
                manufacturer: Default::default(),
                driver: Default::default(),
                interface: Default::default(),
                hw_address: Default::default(),
                uses_progress_bar: Cell::new(true),
                main_graph_color: glib::Bytes::from_static(&super::ResNetwork::MAIN_GRAPH_COLOR),
                icon: RefCell::new(ThemedIcon::new("unknown-network-type-symbolic").into()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("Network Interface"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::new()),
                old_received_bytes: Cell::default(),
                old_sent_bytes: Cell::default(),
                last_timestamp: Cell::new(
                    SystemTime::now()
                        .checked_sub(Duration::from_secs(1))
                        .unwrap(),
                ),
                tab_usage_string: Cell::new(glib::GString::new()),
                graph_locked_max_y: Cell::new(false),
                primary_ord: Cell::new(NETWORK_PRIMARY_ORD),
                secondary_ord: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResNetwork {
        const NAME: &'static str = "ResNetwork";
        type Type = super::ResNetwork;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResNetwork {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }

        fn properties() -> &'static [ParamSpec] {
            Self::derived_properties()
        }

        fn set_property(&self, id: usize, value: &Value, pspec: &ParamSpec) {
            self.derived_set_property(id, value, pspec);
        }

        fn property(&self, id: usize, pspec: &ParamSpec) -> Value {
            self.derived_property(id, pspec)
        }
    }

    impl WidgetImpl for ResNetwork {}
    impl BinImpl for ResNetwork {}
}

glib::wrapper! {
    pub struct ResNetwork(ObjectSubclass<imp::ResNetwork>)
        @extends gtk::Widget, adw::Bin;
}

impl Default for ResNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl ResNetwork {
    // TODO: this is the color for receiving, but it is also used in sidebar,
    // which graphs the sum of send+recv.
    // This does not make much sense, but we probably can't do something
    // like separate send/receive lines without some refactoring to ResGraph.
    const MAIN_GRAPH_COLOR: [u8; 3] = [0x25, 0x9a, 0xab];

    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, network_data: &NetworkData, secondary_ord: u32) {
        self.set_secondary_ord(secondary_ord);
        self.setup_widgets(network_data);
    }

    pub fn setup_widgets(&self, network_data: &NetworkData) {
        let imp = self.imp();
        let network_interface = &network_data.inner;

        let tab_id = format!(
            "{}-{}",
            TAB_ID_PREFIX,
            network_interface.interface_name.to_str().unwrap()
        );
        imp.set_tab_id(&tab_id);

        self.imp().set_icon(&network_interface.icon());

        imp.set_tab_name(&i18n(&network_interface.interface_type.to_string()));

        imp.receiving.set_title_label(&i18n("Receiving"));
        imp.receiving.graph().set_graph_color(0x34, 0xab, 0xaf);
        imp.receiving.graph().set_locked_max_y(None);

        imp.sending.set_title_label(&i18n("Sending"));
        imp.sending.graph().set_graph_color(0x20, 0x81, 0x8f);
        imp.sending.graph().set_locked_max_y(None);

        imp.manufacturer.set_subtitle(
            &network_interface
                .device
                .map(|device| device.vendor().name().to_string())
                .unwrap_or_else(|| i18n("N/A")),
        );

        imp.driver.set_subtitle(
            &network_interface
                .driver_name
                .clone()
                .unwrap_or_else(|| i18n("N/A")),
        );

        imp.interface.set_subtitle(
            network_interface
                .interface_name
                .to_str()
                .unwrap_or(&i18n("N/A")),
        );

        let hw_address = network_interface
            .hw_address
            .clone()
            .unwrap_or_else(|| i18n("N/A"));

        if hw_address.is_empty() {
            imp.hw_address.set_subtitle(&i18n("N/A"));
        } else {
            imp.hw_address.set_subtitle(&hw_address);
        }

        imp.last_timestamp.set(
            SystemTime::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap(),
        );

        imp.old_received_bytes
            .set(network_data.received_bytes.as_ref().ok().copied());
        imp.old_sent_bytes
            .set(network_data.sent_bytes.as_ref().ok().copied());

        imp.set_tab_detail_string(&network_data.display_name);
    }

    pub fn refresh_page(&self, network_data: NetworkData) {
        let NetworkData {
            received_bytes,
            sent_bytes,
            inner: _,
            is_virtual: _,
            display_name: _,
        } = network_data;

        let imp = self.imp();
        let time_passed = SystemTime::now()
            .duration_since(imp.last_timestamp.get())
            .map_or(1.0f64, |timestamp| timestamp.as_secs_f64());

        let (received_delta, received_string) =
            if let (Ok(received_bytes), Some(old_received_bytes)) =
                (received_bytes, imp.old_received_bytes.get())
            {
                let received_delta =
                    (received_bytes.saturating_sub(old_received_bytes)) as f64 / time_passed;

                imp.total_received
                    .set_subtitle(&convert_storage(received_bytes as f64, false));

                imp.receiving.graph().set_visible(true);
                imp.receiving.graph().push_data_point(received_delta);

                let highest_received = imp.receiving.graph().get_highest_value();

                let formatted_delta = convert_speed(received_delta, true);

                imp.receiving.set_subtitle(&format!(
                    "{} · {} {}",
                    &formatted_delta,
                    i18n("Highest:"),
                    convert_speed(highest_received, true)
                ));

                imp.old_received_bytes.set(Some(received_bytes));

                (received_delta, formatted_delta)
            } else {
                imp.total_received.set_subtitle(&i18n("N/A"));

                imp.receiving.graph().set_visible(false);
                imp.receiving.set_subtitle(&i18n("N/A"));

                (0.0, i18n("N/A"))
            };

        let (sent_delta, sent_string) = if let (Ok(sent_bytes), Some(old_sent_bytes)) =
            (sent_bytes, imp.old_sent_bytes.get())
        {
            let sent_delta = (sent_bytes.saturating_sub(old_sent_bytes)) as f64 / time_passed;

            imp.total_sent
                .set_subtitle(&convert_storage(sent_bytes as f64, false));

            imp.sending.graph().set_visible(true);
            imp.sending.graph().push_data_point(sent_delta);

            let highest_sent = imp.sending.graph().get_highest_value();

            let formatted_delta = convert_speed(sent_delta, true);

            imp.sending.set_subtitle(&format!(
                "{} · {} {}",
                &formatted_delta,
                i18n("Highest:"),
                convert_speed(highest_sent, true)
            ));

            imp.old_sent_bytes.set(Some(sent_bytes));

            (sent_delta, formatted_delta)
        } else {
            imp.total_sent.set_subtitle(&i18n("N/A"));

            imp.sending.graph().set_visible(false);
            imp.sending.set_subtitle(&i18n("N/A"));

            (0.0, i18n("N/A"))
        };

        self.set_property("usage", f64::max(received_delta, sent_delta));

        self.set_property(
            "tab_usage_string",
            i18n_f(
                // Translators: This is an abbreviation for "Receive" and "Send". This is displayed in the sidebar so
                // your translation should preferably be quite short or an abbreviation
                "R: {} · S: {}",
                &[&received_string, &sent_string],
            ),
        );

        imp.last_timestamp.set(SystemTime::now());
    }
}
