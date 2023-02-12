use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio::Icon, glib};

use crate::config::PROFILE;

mod imp {
    use super::*;

    use gtk::{Box, CompositeTemplate};

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/me/nalux/Resources/ui/widgets/process_name_cell.ui")]
    pub struct ResProcessNameCell {
        #[template_child]
        pub icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub name: TemplateChild<gtk::Inscription>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResProcessNameCell {
        const NAME: &'static str = "ResProcessNameCell";
        type Type = super::ResProcessNameCell;
        type ParentType = Box;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResProcessNameCell {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.instance();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResProcessNameCell {}

    impl BoxImpl for ResProcessNameCell {}
}

glib::wrapper! {
    pub struct ResProcessNameCell(ObjectSubclass<imp::ResProcessNameCell>)
        @extends gtk::Widget, gtk::Box;
}

impl ResProcessNameCell {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn set_name<S: AsRef<str>>(&self, name: S) {
        let imp = self.imp();
        imp.name.set_text(Some(name.as_ref()));
    }

    pub fn set_icon<P: IsA<Icon>>(&self, gicon: Option<&P>) {
        let imp = self.imp();
        imp.icon.set_gicon(gicon);
    }

    pub fn set_tooltip(&self, tooltip: Option<&str>) {
        let imp = self.imp();
        imp.name.set_tooltip_text(tooltip);
    }
}
