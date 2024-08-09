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
    #[template(resource = "/net/nokyan/Resources/ui/widgets/process_name_cell.ui")]
    #[properties(wrapper_type = super::ResProcessNameCell)]
    pub struct ResProcessNameCell {
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

    impl Default for ResProcessNameCell {
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

    impl ResProcessNameCell {
        pub fn name(&self) -> glib::GString {
            let name = self.name.take();
            self.name.set(name.clone());
            name
        }

        pub fn set_name(&self, name: &str) {
            self.name.set(glib::GString::from(name));
            self.inscription.set_text(Some(name));
        }

        pub fn tooltip(&self) -> glib::GString {
            let tooltip = self.tooltip.take();
            self.tooltip.set(tooltip.clone());
            tooltip
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
                self.image.set_css_classes(&[]);
            } else {
                self.image.set_css_classes(&["lowres-icon"]);
            }
        }
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

    impl WidgetImpl for ResProcessNameCell {}

    impl BoxImpl for ResProcessNameCell {}
}

glib::wrapper! {
    pub struct ResProcessNameCell(ObjectSubclass<imp::ResProcessNameCell>)
        @extends gtk::Widget, gtk::Box;
}

impl Default for ResProcessNameCell {
    fn default() -> Self {
        Self::new()
    }
}

impl ResProcessNameCell {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }
}
