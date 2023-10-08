use std::rc::Rc;

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone};

use super::stack_sidebar_item::ResStackSidebarItem;

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
        rc::Rc,
    };

    use super::*;

    use gtk::{gio, CompositeTemplate, SingleSelection};

    #[derive(CompositeTemplate)]
    #[template(resource = "/net/nokyan/Resources/ui/widgets/stack_sidebar.ui")]
    pub struct ResStackSidebar {
        #[template_child]
        pub list_box: TemplateChild<gtk::ListBox>,

        pub stack: RefCell<Rc<gtk::Stack>>,
        pub pages: RefCell<Rc<gtk::SelectionModel>>,

        pub rows: RefCell<HashMap<gtk::ListBoxRow, gtk::StackPage>>,

        pub populating: Cell<bool>,
    }

    impl Default for ResStackSidebar {
        fn default() -> Self {
            Self {
                list_box: Default::default(),
                stack: Default::default(),
                pages: RefCell::new(Rc::new(SingleSelection::new(None::<gio::ListStore>).into())),
                rows: Default::default(),
                populating: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResStackSidebar {
        const NAME: &'static str = "ResStackSidebar";
        type Type = super::ResStackSidebar;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResStackSidebar {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for ResStackSidebar {}

    impl BinImpl for ResStackSidebar {}

    impl ListBoxRowImpl for ResStackSidebar {}

    impl PreferencesRowImpl for ResStackSidebar {}
}

glib::wrapper! {
    pub struct ResStackSidebar(ObjectSubclass<imp::ResStackSidebar>)
        @extends gtk::Widget;
}

impl ResStackSidebar {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn stack(&self) -> Rc<gtk::Stack> {
        self.imp().stack.borrow().clone()
    }

    fn clear(&self) {
        let imp = self.imp();
        for item in imp.rows.borrow().keys() {
            imp.list_box.remove(item);
        }
        imp.rows.borrow_mut().clear();
    }

    fn populate_list(&self) {
        let imp = self.imp();
        imp.populating.set(true);

        for page in imp
            .stack
            .borrow()
            .pages()
            .iter::<gtk::StackPage>()
            .flatten()
        {
            let child = page
                .child()
                .downcast::<adw::ToolbarView>()
                .unwrap()
                .content()
                .unwrap();

            let sidebar_item =
                ResStackSidebarItem::new(child.property("tab_name"), child.property("icon"));

            child
                .bind_property("tab_name", &sidebar_item, "name")
                .sync_create()
                .build();

            child
                .bind_property("icon", &sidebar_item, "icon")
                .sync_create()
                .build();

            if !child.property::<bool>("uses_progress_bar") {
                sidebar_item.set_progress_bar_visible(false);
            } else {
                child
                    .bind_property("usage", &sidebar_item, "usage")
                    .sync_create()
                    .build();
            }

            let row = gtk::ListBoxRow::builder()
                .child(&sidebar_item)
                .selectable(true)
                .can_focus(true)
                .can_target(true)
                .build();

            imp.list_box.append(&row);

            if let Some(visible_page) = imp.stack.borrow().visible_child() {
                if visible_page == page.child() {
                    imp.list_box.select_row(Some(&row));
                }
            }

            imp.rows.borrow_mut().insert(row, page);
        }

        imp.populating.set(false);
    }

    pub fn set_stack(&self, stack: &gtk::Stack) {
        let imp = self.imp();

        *imp.stack.borrow_mut() = Rc::from(stack.clone());
        *imp.pages.borrow_mut() = Rc::from(stack.clone().pages());

        imp.pages.borrow().connect_items_changed(
            clone!(@strong self as this => move |_, _, _, _| {
                this.clear();
                this.populate_list();
            }),
        );

        imp.list_box.connect_selected_rows_changed(
            clone!(@strong self as this => move |list_box| {
                let imp = this.imp();
                if let Some(selected) = list_box.selected_row() && !imp.populating.get() {
                    imp.stack.borrow().set_visible_child(&imp.rows.borrow().get(&selected).unwrap().child());
                }
            }),
        );
    }
}
