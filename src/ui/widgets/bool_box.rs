use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::{config::PROFILE, i18n::i18n};

mod imp {
    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/widgets/bool_box.ui")]
    pub struct ResBoolBox {
        #[template_child]
        pub info_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResBoolBox {
        const NAME: &'static str = "ResBoolBox";
        type Type = super::ResBoolBox;
        type ParentType = adw::ActionRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResBoolBox {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.instance();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResBoolBox {}

    impl ListBoxRowImpl for ResBoolBox {}

    impl PreferencesRowImpl for ResBoolBox {}

    impl ActionRowImpl for ResBoolBox {}
}

glib::wrapper! {
    pub struct ResBoolBox(ObjectSubclass<imp::ResBoolBox>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow, adw::ActionRow;
}

impl ResBoolBox {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[])
    }

    pub fn set_bool(&self, b: bool) {
        let imp = self.imp();
        if b {
            imp.info_label.set_label(&i18n("Yes"));
        } else {
            imp.info_label.set_label(&i18n("No"));
        }
    }
}
