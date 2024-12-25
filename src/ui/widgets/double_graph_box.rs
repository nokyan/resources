use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;
use log::trace;

use crate::config::PROFILE;

use super::graph::ResGraph;

mod imp {
    use crate::ui::widgets::graph::ResGraph;

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/net/nokyan/Resources/ui/widgets/double_graph_box.ui")]
    pub struct ResDoubleGraphBox {
        #[template_child]
        pub start_graph: TemplateChild<ResGraph>,
        #[template_child]
        pub start_title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub start_info_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub end_graph: TemplateChild<ResGraph>,
        #[template_child]
        pub end_title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub end_info_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResDoubleGraphBox {
        const NAME: &'static str = "ResDoubleGraphBox";
        type Type = super::ResDoubleGraphBox;
        type ParentType = adw::PreferencesRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResDoubleGraphBox {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResDoubleGraphBox {}

    impl ListBoxRowImpl for ResDoubleGraphBox {}

    impl PreferencesRowImpl for ResDoubleGraphBox {}
}

glib::wrapper! {
    pub struct ResDoubleGraphBox(ObjectSubclass<imp::ResDoubleGraphBox>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow;
}

impl Default for ResDoubleGraphBox {
    fn default() -> Self {
        Self::new()
    }
}

impl ResDoubleGraphBox {
    pub fn new() -> Self {
        trace!("Creating ResDoubleGraphBox GObject…");

        glib::Object::new::<Self>()
    }

    pub fn set_graphs_visible(&self, visible: bool) {
        let imp = self.imp();
        imp.start_graph.set_visible(visible);
        imp.end_graph.set_visible(visible);
    }

    pub fn start_graph(&self) -> ResGraph {
        self.imp().start_graph.get()
    }

    pub fn set_start_title_label(&self, str: &str) {
        let imp = self.imp();
        imp.start_title_label.set_label(str);
    }

    pub fn set_start_subtitle(&self, str: &str) {
        let imp = self.imp();
        imp.start_info_label.set_label(str);
    }

    pub fn set_start_tooltip(&self, str: Option<&str>) {
        let imp = self.imp();
        imp.start_info_label.set_tooltip_text(str);
    }

    pub fn end_graph(&self) -> ResGraph {
        self.imp().end_graph.get()
    }

    pub fn set_end_title_label(&self, str: &str) {
        let imp = self.imp();
        imp.end_title_label.set_label(str);
    }

    pub fn set_end_subtitle(&self, str: &str) {
        let imp = self.imp();
        imp.end_info_label.set_label(str);
    }

    pub fn set_end_tooltip(&self, str: Option<&str>) {
        let imp = self.imp();
        imp.end_info_label.set_tooltip_text(str);
    }
}
