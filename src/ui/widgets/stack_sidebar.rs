use std::rc::Rc;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    Ordering,
    glib::{self, GString, clone},
};
use log::trace;
use std::collections::HashMap;

use crate::utils::settings::{SETTINGS, SidebarMeterType};

use super::stack_sidebar_item::ResStackSidebarItem;

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
        rc::Rc,
    };

    use super::*;

    use gtk::{CompositeTemplate, SingleSelection, gio};

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

impl Default for ResStackSidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl ResStackSidebar {
    pub fn new() -> Self {
        trace!("Creating ResStackSidebar GObject…");

        glib::Object::new::<Self>()
    }

    pub fn stack(&self) -> Rc<gtk::Stack> {
        self.imp().stack.borrow().clone()
    }

    fn clear(&self) -> HashMap<String, Vec<f64>> {
        let imp = self.imp();
        let mut return_map = HashMap::with_capacity(imp.rows.borrow().len());

        for item in imp.rows.borrow().keys() {
            let child = item.child().and_downcast::<ResStackSidebarItem>();

            if let Some(child) = child {
                return_map.insert(child.property("tab_id"), child.graph().data_points());
            }

            imp.list_box.remove(item);
        }

        imp.rows.borrow_mut().clear();

        return_map.keys();

        return_map
    }

    fn populate_list(&self, last_data_points: &HashMap<String, Vec<f64>>) {
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

            let sidebar_item = ResStackSidebarItem::new(
                child.property("tab_name"),
                child.property("icon"),
                child.property("tab_detail_string"),
                child.property("tab_usage_string"),
                child.property("graph_locked_max_y"),
                child.property("tab_id"),
                child.property("primary_ord"),
                child.property("secondary_ord"),
            );

            child
                .bind_property("tab_name", &sidebar_item, "name")
                .sync_create()
                .build();

            child
                .bind_property("icon", &sidebar_item, "icon")
                .sync_create()
                .build();

            child
                .bind_property("tab_usage_string", &sidebar_item, "subtitle")
                .sync_create()
                .build();

            child
                .bind_property("tab_detail_string", &sidebar_item, "detail")
                .sync_create()
                .build();

            sidebar_item.set_usage_label_visible(SETTINGS.sidebar_details());
            SETTINGS.connect_sidebar_details(clone!(
                #[weak(rename_to = item)]
                sidebar_item,
                move |sidebar_details| {
                    item.set_usage_label_visible(sidebar_details);
                }
            ));

            sidebar_item.set_detail_label_visible(
                SETTINGS.sidebar_description()
                    && !child.property::<GString>("tab_detail_string").is_empty(),
            );
            SETTINGS.connect_sidebar_description(clone!(
                #[weak(rename_to = item)]
                sidebar_item,
                #[weak]
                child,
                move |sidebar_details| {
                    // if the view doesn't provide a description, disable it regardless of the setting
                    item.set_detail_label_visible(
                        sidebar_details
                            && !child.property::<GString>("tab_detail_string").is_empty(),
                    );
                }
            ));

            // TODO: generalize to "uses_meter"?
            if child.property::<bool>("uses_progress_bar") {
                if child.has_property("main_graph_color", Some(glib::Bytes::static_type())) {
                    let b = child.property::<glib::Bytes>("main_graph_color");
                    sidebar_item.graph().set_graph_color(b[0], b[1], b[2]);
                }

                sidebar_item.set_progress_bar_visible(
                    SETTINGS.sidebar_meter_type() == SidebarMeterType::ProgressBar,
                );
                sidebar_item
                    .graph()
                    .set_visible(SETTINGS.sidebar_meter_type() == SidebarMeterType::Graph);
                SETTINGS.connect_sidebar_meter_type(clone!(
                    #[weak(rename_to = item)]
                    sidebar_item,
                    move |sidebar_meter_type| {
                        item.set_progress_bar_visible(
                            sidebar_meter_type == SidebarMeterType::ProgressBar,
                        );
                        item.graph()
                            .set_visible(sidebar_meter_type == SidebarMeterType::Graph);
                    }
                ));
                child
                    .bind_property("usage", &sidebar_item, "usage")
                    .sync_create()
                    .build();

                if let Some(data) = last_data_points.get(&child.property::<String>("tab_id")) {
                    sidebar_item.graph().clear_data_points();
                    sidebar_item.graph().push_data_points(data);
                }
            } else {
                sidebar_item.set_progress_bar_visible(false);
                sidebar_item.graph().set_visible(false);
            }

            let row = gtk::ListBoxRow::builder()
                .child(&sidebar_item)
                .selectable(true)
                .can_focus(true)
                .can_target(true)
                .build();

            imp.list_box.append(&row);

            row.set_focus_on_click(false);

            if let Some(visible_page) = imp.stack.borrow().visible_child() {
                if visible_page == page.child() {
                    imp.list_box.select_row(Some(&row));
                }
            }

            imp.rows.borrow_mut().insert(row, page);
        }

        imp.populating.set(false);
    }

    pub fn set_selected_list_item_by_tab_id<S: AsRef<str>>(&self, id: S) {
        let imp = self.imp();
        let id = id.as_ref();

        for (row, page) in imp.rows.borrow().iter() {
            let child = page
                .child()
                .downcast::<adw::ToolbarView>()
                .unwrap()
                .content()
                .unwrap();

            if child.property::<GString>("tab_id").as_str() == id {
                imp.list_box.select_row(Some(row));
                break;
            }
        }
    }

    pub fn set_stack(&self, stack: &gtk::Stack) {
        let imp = self.imp();

        *imp.stack.borrow_mut() = Rc::from(stack.clone());
        *imp.pages.borrow_mut() = Rc::from(stack.clone().pages());

        imp.pages.borrow().connect_items_changed(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _, _, _| {
                this.populate_list(&this.clear());
            }
        ));

        imp.list_box.set_sort_func(|a, b| {
            let a_item = a.child().and_downcast::<ResStackSidebarItem>();
            let b_item = b.child().and_downcast::<ResStackSidebarItem>();

            if let (Some(a_item), Some(b_item)) = (a_item, b_item) {
                a_item.ord(&b_item)
            } else {
                Ordering::Equal
            }
        });

        imp.list_box.connect_selected_rows_changed(clone!(
            #[weak(rename_to = this)]
            self,
            move |list_box| {
                let imp = this.imp();
                if let Some(selected) = list_box.selected_row() {
                    if !imp.populating.get() {
                        let child = imp.rows.borrow().get(&selected).unwrap().child();

                        imp.stack.borrow().set_visible_child(&child);

                        if let Some(page) = child
                            .downcast_ref::<adw::ToolbarView>()
                            .and_then(adw::ToolbarView::content)
                        {
                            let _ = SETTINGS
                                .set_last_viewed_page(page.property::<GString>("tab-id").as_str());
                        }
                    }
                }
            }
        ));
    }
}
