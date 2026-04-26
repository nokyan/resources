use core::f64;

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;
use log::trace;

use crate::{
    config::PROFILE,
    i18n::i18n,
    utils::{
        FiniteOr,
        units::{
            convert_fraction, convert_power, convert_speed, convert_storage, convert_temperature,
        },
    },
};

use super::graph::ResGraph;

mod imp {
    use crate::ui::widgets::graph::ResGraph;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/net/nokyan/Resources/ui/widgets/graph_box.ui")]
    pub struct ResGraphBox {
        #[template_child]
        pub graph: TemplateChild<ResGraph>,
        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub info_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResGraphBox {
        const NAME: &'static str = "ResGraphBox";
        type Type = super::ResGraphBox;
        type ParentType = adw::PreferencesRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResGraphBox {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResGraphBox {}

    impl ListBoxRowImpl for ResGraphBox {}

    impl PreferencesRowImpl for ResGraphBox {}
}

glib::wrapper! {
    pub struct ResGraphBox(ObjectSubclass<imp::ResGraphBox>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Accessible, gtk::Actionable;
}

impl Default for ResGraphBox {
    fn default() -> Self {
        Self::new()
    }
}

impl ResGraphBox {
    pub fn new() -> Self {
        trace!("Creating ResGraphBox GObject…");

        glib::Object::new::<Self>()
    }

    pub fn graph(&self) -> ResGraph {
        self.imp().graph.get()
    }

    pub fn set_title_label(&self, str: &str) {
        let imp = self.imp();
        imp.title_label.set_label(str);
    }

    pub fn set_subtitle(&self, str: &str) {
        let imp = self.imp();
        imp.info_label.set_label(str);
    }

    pub fn set_tooltip(&self, str: Option<&str>) {
        let imp = self.imp();
        imp.info_label.set_tooltip_text(str);
    }

    fn add_maybe_clamped_point<F: Fn(f64) -> String>(
        &self,
        value: Option<f64>,
        max_value: Option<f64>,
        stringify_fn: F,
    ) -> String {
        match (value, max_value) {
            (Some(value), Some(max_value)) => {
                let fraction = (value as f64 / max_value as f64).finite_or_default();

                let percentage_string = convert_fraction(fraction as f64, true);

                let subtitle = format!(
                    "{} / {} · {}",
                    stringify_fn(value as f64),
                    stringify_fn(max_value as f64),
                    percentage_string
                );

                self.graph().set_visible(true);
                self.set_subtitle(&subtitle);
                self.graph().push_data_point(fraction);
                self.graph().set_locked_max_y(Some(1.0));

                subtitle
            }
            (Some(used_bytes), None) => {
                self.add_unclamped_point(Some(used_bytes as f64), |bytes| stringify_fn(bytes))
            }
            _ => {
                self.set_subtitle(&i18n("N/A"));
                self.graph().set_visible(false);

                i18n("N/A")
            }
        }
    }

    fn add_unclamped_point<F: Fn(f64) -> String>(
        &self,
        value: Option<f64>,
        stringify_fn: F,
    ) -> String {
        self.graph().set_visible(value.is_some());
        if let Some(value) = value {
            let value_string = stringify_fn(value);

            let highest_value_string = stringify_fn(self.graph().get_highest_value());

            let subtitle = format!(
                "{} · {} {}",
                &value_string,
                i18n("Highest:"),
                highest_value_string
            );

            self.set_subtitle(&subtitle);
            self.graph().push_data_point(value);
            self.graph().set_locked_max_y(None);

            value_string
        } else {
            self.set_subtitle(&i18n("N/A"));

            i18n("N/A")
        }
    }

    pub fn add_fraction_point(&self, fraction: Option<f64>) -> String {
        self.graph().set_visible(fraction.is_some());
        if let Some(fraction) = fraction {
            let subtitle = convert_fraction(fraction, true);

            self.set_subtitle(&subtitle);
            self.graph().push_data_point(fraction);
            self.graph().set_locked_max_y(Some(1.0));

            subtitle
        } else {
            self.set_subtitle(&i18n("N/A"));

            i18n("N/A")
        }
    }

    pub fn add_storage_point(&self, used_bytes: Option<u64>, total_bytes: Option<u64>) -> String {
        self.add_maybe_clamped_point(
            used_bytes.map(|bytes| bytes as f64),
            total_bytes.map(|bytes| bytes as f64),
            |bytes| convert_storage(bytes, false),
        )
    }

    pub fn add_temperature_point(&self, temperature: Option<f64>) -> String {
        self.add_unclamped_point(temperature, convert_temperature)
    }

    pub fn add_power_point(&self, power_usage: Option<f64>, max_power: Option<f64>) -> String {
        self.add_maybe_clamped_point(power_usage, max_power, convert_power)
    }

    pub fn add_speed_point(&self, bytes_per_second: Option<f64>) -> String {
        self.add_unclamped_point(bytes_per_second, |bps| convert_speed(bps, false))
    }

    pub fn add_speed_point_network(&self, bytes_per_second: Option<f64>) -> String {
        self.add_unclamped_point(bytes_per_second, |bps| convert_speed(bps, true))
    }
}
