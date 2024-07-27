use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio::Icon, glib};

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    use gtk::{
        gio::ThemedIcon,
        glib::{ParamSpec, Properties, Value},
        Box, CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/widgets/application_name_cell.ui")]
    #[properties(wrapper_type = super::ResApplicationNameCell)]
    pub struct ResApplicationNameCell {
        #[template_child]
        pub image: TemplateChild<gtk::Image>,
        #[template_child]
        pub inscription: TemplateChild<gtk::Inscription>,

        #[property(get = Self::name, set = Self::set_name, type = glib::GString)]
        name: Cell<glib::GString>,
        #[property(get = Self::tooltip, set = Self::set_tooltip, type = glib::GString)]
        tooltip: Cell<glib::GString>,
        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: RefCell<Icon>,
        #[property(get, set = Self::set_symbolic)]
        symbolic: Cell<bool>,
    }

    impl Default for ResApplicationNameCell {
        fn default() -> Self {
            Self {
                image: Default::default(),
                inscription: Default::default(),
                name: Default::default(),
                tooltip: Default::default(),
                icon: RefCell::new(ThemedIcon::new("generic-process").into()),
                symbolic: Default::default(),
            }
        }
    }

    impl ResApplicationNameCell {
        pub fn name(&self) -> glib::GString {
            let name = self.name.take();
            let result = name.clone();
            self.name.set(name);

            result
        }

        pub fn set_name(&self, name: &str) {
            self.name.set(glib::GString::from(name));
            self.inscription.set_text(Some(name));
        }

        pub fn tooltip(&self) -> glib::GString {
            let tooltip = self.tooltip.take();
            let result = tooltip.clone();
            self.tooltip.set(tooltip);

            result
        }

        pub fn set_tooltip(&self, tooltip: &str) {
            self.tooltip.set(glib::GString::from(tooltip));
            self.inscription.set_tooltip_text(Some(tooltip));
        }

        pub fn icon(&self) -> Icon {
            let icon = self
                .icon
                .replace_with(|_| ThemedIcon::new("generic-process").into());
            self.icon.set(icon.clone());

            icon
        }

        pub fn set_icon(&self, icon: &Icon) {
            let current_icon = self
                .icon
                .replace_with(|_| ThemedIcon::new("generic-process").into());

            if &current_icon == icon {
                self.icon.set(current_icon);
                return;
            }

            self.image.set_from_gicon(icon);

            self.icon.set(icon.clone());
        }

        pub fn set_symbolic(&self, symbolic: bool) {
            self.symbolic.set(symbolic);

            if symbolic {
                self.image.set_css_classes(&["bubble"]);
                self.image.set_pixel_size(16);
            } else {
                self.image.set_css_classes(&["lowres-icon"]);
                self.image.set_pixel_size(32);
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResApplicationNameCell {
        const NAME: &'static str = "ResApplicationNameCell";
        type Type = super::ResApplicationNameCell;
        type ParentType = Box;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResApplicationNameCell {
        fn constructed(&self) {
            self.parent_constructed();
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

    impl WidgetImpl for ResApplicationNameCell {}

    impl BoxImpl for ResApplicationNameCell {}
}

glib::wrapper! {
    pub struct ResApplicationNameCell(ObjectSubclass<imp::ResApplicationNameCell>)
        @extends gtk::Widget, gtk::Box;
}

impl Default for ResApplicationNameCell {
    fn default() -> Self {
        Self::new()
    }
}

impl ResApplicationNameCell {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }
}
