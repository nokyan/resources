use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::config::PROFILE;

mod imp {
    use crate::ui::widgets::graph::ResGraph;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/widgets/graph_box.ui")]
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
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow;
}

impl ResGraphBox {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn set_data_points_max_amount(&self, max_amount: usize) {
        let imp = self.imp();
        imp.graph.set_data_points_max_amount(max_amount);
    }

    pub fn set_graph_color(&self, r: u8, g: u8, b: u8) {
        let imp = self.imp();
        imp.graph.set_graph_color(r, g, b);
    }

    pub fn set_graph_visible(&self, visible: bool) {
        let imp = self.imp();
        imp.graph.set_visible(visible);
    }

    pub fn push_data_point(&self, data: f64) {
        let imp = self.imp();
        imp.graph.push_data_point(data);
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

    pub fn set_graph_height_request(&self, height_request: i32) {
        let imp = self.imp();
        imp.graph.set_height_request(height_request);
    }

    pub fn set_locked_max_y(&self, y_max: Option<f64>) {
        let imp = self.imp();
        imp.graph.set_locked_max_y(y_max);
    }

    pub fn get_highest_value(&self) -> f64 {
        let imp = self.imp();
        imp.graph.get_highest_value()
    }
}
