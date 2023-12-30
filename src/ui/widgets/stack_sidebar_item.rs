use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    gio::Icon,
    glib::{self},
};

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/widgets/stack_sidebar_item.ui")]
    #[properties(wrapper_type = super::ResStackSidebarItem)]
    pub struct ResStackSidebarItem {
        #[template_child]
        pub image: TemplateChild<gtk::Image>,
        #[template_child]
        pub label: TemplateChild<gtk::Label>,
        #[template_child]
        pub subtitle_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub progress_bar: TemplateChild<gtk::ProgressBar>,
        #[template_child]
        pub graph: TemplateChild<super::super::graph::ResGraph>,

        #[property(get = Self::name, set = Self::set_name, type = glib::GString)]
        name: Cell<glib::GString>,
        #[property(get = Self::subtitle, set = Self::set_subtitle, type = glib::GString)]
        subtitle: Cell<glib::GString>,
        #[property(get = Self::icon, set = Self::set_icon, type = Icon)]
        icon: RefCell<Icon>,
        #[property(get, set = Self::set_usage)]
        usage: Cell<f64>,
    }

    impl ResStackSidebarItem {
        pub fn name(&self) -> glib::GString {
            let name = self.name.take();
            let result = name.clone();
            self.name.set(name);

            result
        }

        pub fn set_name(&self, name: &str) {
            let current_name = self.name.take();
            if current_name.as_str() == name {
                self.name.set(current_name);
                return;
            }
            self.name.set(glib::GString::from(name));
            self.label.set_label(name);
        }

        pub fn subtitle(&self) -> glib::GString {
            let subtitle = self.subtitle.take();
            let result = subtitle.clone();
            self.subtitle.set(subtitle);

            result
        }

        pub fn set_subtitle(&self, subtitle: &str) {
            let current_subtitle = self.subtitle.take();
            if current_subtitle.as_str() == subtitle {
                self.subtitle.set(current_subtitle);
                return;
            }
            self.name.set(glib::GString::from(subtitle));
            self.subtitle_label.set_label(subtitle);
        }

        pub fn icon(&self) -> Icon {
            let icon = self
                .icon
                .replace_with(|_| ThemedIcon::new("generic-process").into());
            let result = icon.clone();
            self.icon.set(icon);
            result
        }

        pub fn set_icon(&self, icon: &Icon) {
            self.image.set_gicon(Some(icon));
            self.icon.set(icon.clone());
        }

        pub fn set_usage(&self, usage: f64) {
            self.usage.set(usage);
            self.progress_bar.set_fraction(usage);
            self.graph.push_data_point(usage);
        }
    }

    impl Default for ResStackSidebarItem {
        fn default() -> Self {
            Self {
                image: Default::default(),
                label: Default::default(),
                progress_bar: Default::default(),
                graph: Default::default(),
                subtitle_label: Default::default(),
                name: Default::default(),
                subtitle: Default::default(),
                icon: RefCell::new(ThemedIcon::new("generic-process").into()),
                usage: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResStackSidebarItem {
        const NAME: &'static str = "ResStackSidebarItem";
        type Type = super::ResStackSidebarItem;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResStackSidebarItem {
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

    impl WidgetImpl for ResStackSidebarItem {}

    impl BinImpl for ResStackSidebarItem {}
}

glib::wrapper! {
    pub struct ResStackSidebarItem(ObjectSubclass<imp::ResStackSidebarItem>)
        @extends adw::Bin, gtk::Widget;
}

impl ResStackSidebarItem {
    pub fn new(name: String, icon: Icon, subtitle: String) -> Self {
        let this: Self = glib::Object::builder()
            .property("name", &name)
            .property("icon", icon)
            .property("subtitle", subtitle)
            .build();
        this.imp().graph.set_height_request(64);
        this
    }

    pub fn set_progress_bar_visible(&self, visible: bool) {
        self.imp().progress_bar.set_visible(visible);
    }

    pub fn set_graph_visible(&self, visible: bool) {
        self.imp().graph.set_visible(visible);
    }

    pub fn set_graph_color(&self, r: u8, g: u8, b: u8) {
        self.imp().graph.set_graph_color(r, g, b);
    }

    pub fn set_subtitle_visible(&self, visible: bool) {
        self.imp().subtitle_label.set_visible(visible);
    }
}
