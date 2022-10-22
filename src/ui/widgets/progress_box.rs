use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::config::PROFILE;

mod imp {
    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/widgets/progress_box.ui")]
    pub struct ResProgressBox {
        #[template_child]
        pub percentage_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub progress_bar: TemplateChild<gtk::ProgressBar>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResProgressBox {
        const NAME: &'static str = "ResProgressBox";
        type Type = super::ResProgressBox;
        type ParentType = adw::ActionRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResProgressBox {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResProgressBox {}

    impl ListBoxRowImpl for ResProgressBox {}

    impl PreferencesRowImpl for ResProgressBox {}

    impl ActionRowImpl for ResProgressBox {}
}

glib::wrapper! {
    pub struct ResProgressBox(ObjectSubclass<imp::ResProgressBox>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow, adw::ActionRow;
}

impl ResProgressBox {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ResProgressBox")
    }

    pub fn set_fraction(&self, fraction: f64) {
        let imp = self.imp();
        imp.progress_bar.set_fraction(fraction);
    }

    pub fn set_percentage_label(&self, str: &str) {
        let imp = self.imp();
        imp.percentage_label.set_label(str);
    }

    pub fn set_progressbar_visible(&self, visible: bool) {
        let imp = self.imp();
        imp.progress_bar.set_visible(visible)
    }
}
