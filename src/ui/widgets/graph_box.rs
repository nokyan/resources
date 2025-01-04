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
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow;
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
}
