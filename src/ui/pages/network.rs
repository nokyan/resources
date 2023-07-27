use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, timeout_future_seconds, MainContext};

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::widgets::info_box::ResInfoBox;
use crate::utils::network::NetworkInterface;
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use crate::ui::widgets::graph_box::ResGraphBox;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/pages/network.ui")]
    pub struct ResNetwork {
        #[template_child]
        pub interface_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub receiving: TemplateChild<ResGraphBox>,
        #[template_child]
        pub sending: TemplateChild<ResGraphBox>,
        #[template_child]
        pub total_received: TemplateChild<ResInfoBox>,
        #[template_child]
        pub total_sent: TemplateChild<ResInfoBox>,
        #[template_child]
        pub manufacturer: TemplateChild<ResInfoBox>,
        #[template_child]
        pub driver: TemplateChild<ResInfoBox>,
        #[template_child]
        pub interface: TemplateChild<ResInfoBox>,
        #[template_child]
        pub hw_address: TemplateChild<ResInfoBox>,
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
        imp.interface_name
            .set_label(&network_interface.display_name());
        imp.manufacturer
            .set_info_label(&network_interface.vendor.unwrap_or_else(|| i18n("N/A")));
        imp.driver
            .set_info_label(&network_interface.driver_name.unwrap_or_else(|| i18n("N/A")));
        imp.interface.set_info_label(
            network_interface
                .interface_name
                .to_str()
                .unwrap_or(&i18n("N/A")),
        );
        imp.hw_address
            .set_info_label(&network_interface.hw_address.unwrap_or_else(|| i18n("N/A")));
    }

    pub fn setup_listener(&self, network_interface: NetworkInterface) {
        let main_context = MainContext::default();
        let statistics_update = clone!(@strong self as this => async move {
            let imp = this.imp();

            let mut old_received_bytes = network_interface.received_bytes().await.unwrap_or(0);
            let mut old_sent_bytes = network_interface.sent_bytes().await.unwrap_or(0);

            loop {
                let received_bytes = network_interface.received_bytes().await.unwrap_or(0);
                let sent_bytes = network_interface.sent_bytes().await.unwrap_or(0);
                let received_delta = received_bytes - old_received_bytes;
                let sent_delta = sent_bytes - old_sent_bytes;

                let received_delta_formatted = to_largest_unit(received_delta as f64, &Base::Decimal);
                let sent_delta_formatted = to_largest_unit(sent_delta as f64, &Base::Decimal);
                let received_formatted = to_largest_unit(received_bytes as f64, &Base::Decimal);
                let sent_formatted = to_largest_unit(sent_bytes as f64, &Base::Decimal);
                imp.total_received.set_info_label(&format!("{:.2} {}B", received_formatted.0, received_formatted.1));
                imp.total_sent.set_info_label(&format!("{:.2} {}B", sent_formatted.0, sent_formatted.1));
                imp.receiving.push_data_point(received_delta as f64);
                let highest_received = to_largest_unit(imp.receiving.get_highest_value(), &Base::Decimal);
                imp.receiving.set_info_label(&format!("{:.2} {}B/s · {} {:.2} {}B/s", received_delta_formatted.0, received_delta_formatted.1, i18n("Highest:"), highest_received.0, highest_received.1));
                imp.sending.push_data_point(sent_delta as f64);
                let highest_sent = to_largest_unit(imp.sending.get_highest_value(), &Base::Decimal);
                imp.sending.set_info_label(&format!("{:.2} {}B/s · {} {:.2} {}B/s", sent_delta_formatted.0, sent_delta_formatted.1, i18n("Highest:"), highest_sent.0, highest_sent.1));

                old_received_bytes = received_bytes;
                old_sent_bytes = sent_bytes;

                timeout_future_seconds(1).await;
            }
        });
        main_context.spawn_local(statistics_update);
    }
}
