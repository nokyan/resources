use std::time::{Duration, SystemTime};

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, timeout_future_seconds, MainContext};

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::utils::network::NetworkInterface;
use crate::utils::units::{to_largest_unit, Base};
use crate::utils::NaNDefault;

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::ui::widgets::graph_box::ResGraphBox;

    use super::*;

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/me/nalux/Resources/ui/pages/network.ui")]
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

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: RefCell<Icon>,

        #[property(get, set)]
        usage: Cell<f64>,

        #[property(get = Self::tab_name, set = Self::set_tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,
    }

    impl ResNetwork {
        pub fn tab_name(&self) -> glib::GString {
            let tab_name = self.tab_name.take();
            let result = tab_name.clone();
            self.tab_name.set(tab_name);
            result
        }

        pub fn set_tab_name(&self, tab_name: &str) {
            self.tab_name.set(glib::GString::from(tab_name));
        }

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
                icon: RefCell::new(ThemedIcon::new("unknown-network-type-symbolic").into()),
                usage: Default::default(),
                tab_name: Cell::new(glib::GString::from(i18n("Network Interface"))),
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

impl ResNetwork {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, network_interface: NetworkInterface) {
        self.imp().set_icon(&network_interface.icon());
        self.setup_widgets(network_interface.clone());
        self.setup_listener(network_interface);
    }

    pub fn setup_widgets(&self, network_interface: NetworkInterface) {
        let imp = self.imp();
        imp.receiving.set_title_label(&i18n("Receiving"));
        imp.receiving.set_graph_color(52, 170, 175);
        imp.receiving.set_data_points_max_amount(60);
        imp.receiving.set_locked_max_y(None);
        imp.sending.set_title_label(&i18n("Sending"));
        imp.sending.set_graph_color(222, 77, 119);
        imp.sending.set_data_points_max_amount(60);
        imp.sending.set_locked_max_y(None);
        imp.manufacturer
            .set_subtitle(&network_interface.vendor.unwrap_or_else(|| i18n("N/A")));
        imp.driver
            .set_subtitle(&network_interface.driver_name.unwrap_or_else(|| i18n("N/A")));
        imp.interface.set_subtitle(
            network_interface
                .interface_name
                .to_str()
                .unwrap_or(&i18n("N/A")),
        );
        let hw_address = network_interface.hw_address.unwrap_or_else(|| i18n("N/A"));
        if hw_address.is_empty() {
            imp.hw_address.set_subtitle(&i18n("N/A"));
        } else {
            imp.hw_address.set_subtitle(&hw_address);
        }
    }

    pub fn setup_listener(&self, network_interface: NetworkInterface) {
        let main_context = MainContext::default();
        let statistics_update = clone!(@strong self as this => async move {
            let imp = this.imp();

            let mut old_received_bytes = network_interface.received_bytes().await.unwrap_or(0);
            let mut old_sent_bytes = network_interface.sent_bytes().await.unwrap_or(0);

            let mut last_timestamp = SystemTime::now().checked_sub(Duration::from_secs(1)).unwrap();

            loop {
                let time_passed = SystemTime::now().duration_since(last_timestamp).map_or(1.0f64, |timestamp| timestamp.as_secs_f64());

                let received_bytes = network_interface.received_bytes().await.unwrap_or(0);
                let sent_bytes = network_interface.sent_bytes().await.unwrap_or(0);
                let received_delta = (received_bytes - old_received_bytes) as f64 / time_passed;
                let sent_delta = (sent_bytes - old_sent_bytes) as f64 / time_passed;

                let received_delta_formatted = to_largest_unit(received_delta, &Base::Decimal);
                let sent_delta_formatted = to_largest_unit(sent_delta, &Base::Decimal);
                let received_formatted = to_largest_unit(received_bytes as f64, &Base::Decimal);
                let sent_formatted = to_largest_unit(sent_bytes as f64, &Base::Decimal);

                imp.total_received.set_subtitle(&format!("{:.2} {}B", received_formatted.0, received_formatted.1));
                imp.total_sent.set_subtitle(&format!("{:.2} {}B", sent_formatted.0, sent_formatted.1));

                imp.receiving.push_data_point(received_delta as f64);
                let highest_received = imp.receiving.get_highest_value();
                let highest_received_formatted = to_largest_unit(imp.receiving.get_highest_value(), &Base::Decimal);
                imp.receiving.set_subtitle(&format!("{:.2} {}B/s · {} {:.2} {}B/s", received_delta_formatted.0, received_delta_formatted.1, i18n("Highest:"), highest_received_formatted.0, highest_received_formatted.1));

                imp.sending.push_data_point(sent_delta as f64);
                let highest_sent = imp.sending.get_highest_value();
                let highest_sent_formatted = to_largest_unit(imp.sending.get_highest_value(), &Base::Decimal);
                imp.sending.set_subtitle(&format!("{:.2} {}B/s · {} {:.2} {}B/s", sent_delta_formatted.0, sent_delta_formatted.1, i18n("Highest:"), highest_sent_formatted.0, highest_sent_formatted.1));

                this.set_property("usage", f64::max(received_delta / highest_received, sent_delta / highest_sent).nan_default(1.0));

                old_received_bytes = received_bytes;
                old_sent_bytes = sent_bytes;
                last_timestamp = SystemTime::now();

                timeout_future_seconds(1).await;
            }
        });
        main_context.spawn_local(statistics_update);
    }
}
