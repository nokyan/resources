mod application_entry;
mod application_name_cell;

use std::collections::HashSet;

use adw::ResponseAppearance;
use adw::{prelude::*, subclass::prelude::*};
use async_channel::Sender;
use gtk::glib::{self, clone, closure, MainContext, Object};
use gtk::{
    gio, ColumnView, ColumnViewColumn, EventControllerKey, FilterChange, NumericSorter, SortType,
    StringSorter, Widget,
};

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::ui::dialogs::app_dialog::ResAppDialog;
use crate::ui::window::{Action, MainWindow};
use crate::utils::app::{AppItem, AppsContext};
use crate::utils::process::ProcessAction;
use crate::utils::settings::SETTINGS;
use crate::utils::units::{convert_speed, convert_storage};
use crate::utils::NUM_CPUS;

use self::application_entry::ApplicationEntry;
use self::application_name_cell::ResApplicationNameCell;

pub const TAB_ID: &str = "applications";

mod imp {
    use std::{
        cell::{Cell, RefCell},
        sync::OnceLock,
    };

    use crate::ui::window::Action;

    use super::*;

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
        ColumnViewColumn, CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/applications.ui")]
    #[properties(wrapper_type = super::ResApplications)]
    pub struct ResApplications {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub search_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub applications_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub search_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub information_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub end_application_button: TemplateChild<adw::SplitButton>,

        pub store: RefCell<gio::ListStore>,
        pub selection_model: RefCell<gtk::SingleSelection>,
        pub filter_model: RefCell<gtk::FilterListModel>,
        pub sort_model: RefCell<gtk::SortListModel>,
        pub column_view: RefCell<gtk::ColumnView>,
        pub open_dialog: RefCell<Option<(Option<String>, ResAppDialog)>>,

        pub sender: OnceLock<Sender<Action>>,

        pub popped_over_app: RefCell<Option<ApplicationEntry>>,

        pub columns: RefCell<Vec<ColumnViewColumn>>,

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get)]
        icon: RefCell<Icon>,

        #[property(get = Self::tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,

        #[property(get = Self::tab_detail, type = glib::GString)]
        tab_detail_string: Cell<glib::GString>,

        #[property(get = Self::tab_usage_string, set = Self::set_tab_usage_string, type = glib::GString)]
        tab_usage_string: Cell<glib::GString>,

        #[property(get = Self::tab_id, type = glib::GString)]
        tab_id: Cell<glib::GString>,

        #[property(get)]
        graph_locked_max_y: Cell<bool>,
    }

    impl ResApplications {
        pub fn tab_name(&self) -> glib::GString {
            let tab_name = self.tab_name.take();
            let result = tab_name.clone();
            self.tab_name.set(tab_name);
            result
        }

        pub fn tab_detail(&self) -> glib::GString {
            let detail = self.tab_detail_string.take();
            let result = detail.clone();
            self.tab_detail_string.set(detail);
            result
        }

        pub fn tab_usage_string(&self) -> glib::GString {
            let tab_usage_string = self.tab_usage_string.take();
            let result = tab_usage_string.clone();
            self.tab_usage_string.set(tab_usage_string);
            result
        }

        pub fn set_tab_usage_string(&self, tab_usage_string: &str) {
            self.tab_usage_string
                .set(glib::GString::from(tab_usage_string));
        }

        pub fn tab_id(&self) -> glib::GString {
            let tab_id = self.tab_id.take();
            let result = tab_id.clone();
            self.tab_id.set(tab_id);
            result
        }
    }

    impl Default for ResApplications {
        fn default() -> Self {
            Self {
                toast_overlay: Default::default(),
                search_revealer: Default::default(),
                search_entry: Default::default(),
                search_button: Default::default(),
                information_button: Default::default(),
                store: gio::ListStore::new::<ApplicationEntry>().into(),
                selection_model: Default::default(),
                filter_model: Default::default(),
                sort_model: Default::default(),
                column_view: Default::default(),
                open_dialog: Default::default(),
                sender: Default::default(),
                applications_scrolled_window: Default::default(),
                end_application_button: Default::default(),
                uses_progress_bar: Cell::new(false),
                icon: RefCell::new(ThemedIcon::new("app-symbolic").into()),
                tab_name: Cell::from(glib::GString::from(i18n("Apps"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_usage_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::from(TAB_ID)),
                popped_over_app: Default::default(),
                columns: Default::default(),
                graph_locked_max_y: Cell::new(true),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResApplications {
        const NAME: &'static str = "ResApplications";
        type Type = super::ResApplications;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action(
                "applications.context-end-process",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        if let Some(app_item) = application_entry.app_item() {
                            res_applications
                                .execute_process_action_dialog(app_item, ProcessAction::TERM);
                        }
                    }
                },
            );

            klass.install_action(
                "applications.context-kill-process",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        if let Some(app_item) = application_entry.app_item() {
                            res_applications
                                .execute_process_action_dialog(app_item, ProcessAction::KILL);
                        }
                    }
                },
            );

            klass.install_action(
                "applications.context-halt-process",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        if let Some(app_item) = application_entry.app_item() {
                            res_applications
                                .execute_process_action_dialog(app_item, ProcessAction::STOP);
                        }
                    }
                },
            );

            klass.install_action(
                "applications.context-continue-process",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        if let Some(app_item) = application_entry.app_item() {
                            res_applications
                                .execute_process_action_dialog(app_item, ProcessAction::CONT);
                        }
                    }
                },
            );

            klass.install_action(
                "applications.context-information",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        res_applications
                            .open_information_dialog(&application_entry.app_item().unwrap());
                    }
                },
            );

            klass.install_action(
                "applications.kill-application",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_item() {
                        res_applications.execute_process_action_dialog(app, ProcessAction::KILL);
                    }
                },
            );

            klass.install_action(
                "applications.halt-application",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_item() {
                        res_applications.execute_process_action_dialog(app, ProcessAction::STOP);
                    }
                },
            );

            klass.install_action(
                "applications.continue-application",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_item() {
                        res_applications.execute_process_action_dialog(app, ProcessAction::CONT);
                    }
                },
            );

            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResApplications {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
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

    impl WidgetImpl for ResApplications {}
    impl BinImpl for ResApplications {}
}

glib::wrapper! {
    pub struct ResApplications(ObjectSubclass<imp::ResApplications>)
        @extends gtk::Widget, adw::Bin;
}

impl ResApplications {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn toggle_search(&self) {
        let imp = self.imp();
        imp.search_button.set_active(!imp.search_button.is_active());
    }

    pub fn close_search(&self) {
        let imp = self.imp();
        imp.search_button.set_active(false);
    }

    pub fn init(&self, sender: Sender<Action>) {
        let imp = self.imp();
        imp.sender.set(sender).unwrap();

        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();

        *imp.column_view.borrow_mut() = gtk::ColumnView::new(None::<gtk::SingleSelection>);
        let column_view = imp.column_view.borrow();

        let mut columns = imp.columns.borrow_mut();

        columns.push(Self::add_name_column(&column_view));
        columns.push(Self::add_memory_column(&column_view));
        columns.push(Self::add_cpu_column(&column_view));
        columns.push(Self::add_read_speed_column(&column_view));
        columns.push(Self::add_read_total_column(&column_view));
        columns.push(Self::add_write_speed_column(&column_view));
        columns.push(Self::add_write_total_column(&column_view));
        columns.push(Self::add_gpu_column(&column_view));
        columns.push(Self::add_gpu_mem_column(&column_view));
        columns.push(Self::add_encoder_column(&column_view));
        columns.push(Self::add_decoder_column(&column_view));

        let store = gio::ListStore::new::<ApplicationEntry>();

        let filter_model = gtk::FilterListModel::new(
            Some(store.clone()),
            Some(gtk::CustomFilter::new(
                clone!(@strong self as this => move |obj| this.search_filter(obj)),
            )),
        );

        let sort_model = gtk::SortListModel::new(Some(filter_model.clone()), column_view.sorter());

        let selection_model = gtk::SingleSelection::new(Some(sort_model.clone()));
        selection_model.set_can_unselect(true);
        selection_model.set_autoselect(false);

        column_view.set_model(Some(&selection_model));

        column_view.sort_by_column(
            columns
                .get(SETTINGS.apps_sort_by() as usize)
                .or_else(|| columns.first()),
            SETTINGS.apps_sort_by_ascending(),
        );

        column_view.add_css_class("resources-columnview");

        *imp.store.borrow_mut() = store;
        *imp.selection_model.borrow_mut() = selection_model;
        *imp.sort_model.borrow_mut() = sort_model;
        *imp.filter_model.borrow_mut() = filter_model;

        imp.applications_scrolled_window
            .set_child(Some(&*column_view));
    }

    pub fn setup_signals(&self) {
        let imp = self.imp();

        imp.selection_model.borrow().connect_selection_changed(
        clone!(@strong self as this => move |model, _, _| {
            let imp = this.imp();
                let is_system_processes = model.selected_item().map_or(false, |object| {
                    object
                    .downcast::<ApplicationEntry>()
                    .unwrap()
                    .id()
                    .is_none()
                });
                imp.information_button.set_sensitive(model.selected() != u32::MAX);
                imp.end_application_button.set_sensitive(model.selected() != u32::MAX && !is_system_processes);
            }),
        );

        imp.search_button
            .connect_toggled(clone!(@strong self as this => move |button| {
                let imp = this.imp();
                imp.search_revealer.set_reveal_child(button.is_active());
                if let Some(filter) = imp.filter_model.borrow().filter() {
                    filter.changed(FilterChange::Different);
                }
                if button.is_active() {
                    imp.search_entry.grab_focus();
                }
            }));

        imp.search_entry
            .connect_search_changed(clone!(@strong self as this => move |_| {
            let imp = this.imp();
                if let Some(filter) = imp.filter_model.borrow().filter() {
                    filter.changed(FilterChange::Different);
                }
            }));

        let event_controller = EventControllerKey::new();
        event_controller.connect_key_released(clone!(@strong self as this => move|_, key, _, _| {
            if key.name().unwrap_or_default() == "Escape" {
                this.close_search()
            }
        }));
        imp.search_entry.add_controller(event_controller);

        imp.information_button
            .connect_clicked(clone!(@strong self as this => move |_| {
                let imp = this.imp();
                let selection_option = imp.selection_model.borrow()
                .selected_item()
                .map(|object| {
                    object
                    .downcast::<ApplicationEntry>()
                    .unwrap()
                });
                if let Some(selection) = selection_option {
                    this.open_information_dialog(&selection.app_item().unwrap());
                }
            }));

        imp.end_application_button
            .connect_clicked(clone!(@strong self as this => move |_| {
                if let Some(app) = this.get_selected_app_item() {
                    this.execute_process_action_dialog(app, ProcessAction::TERM);
                }
            }));

        if let Some(column_view_sorter) = imp.column_view.borrow().sorter() {
            column_view_sorter.connect_changed(clone!(@strong self as this => move |sorter, _| {
                if let Some(sorter) = sorter.downcast_ref::<gtk::ColumnViewSorter>() {
                    let current_column = sorter.primary_sort_column().map(|column| column.as_ptr() as usize).unwrap_or_default();

                    let current_column_number = this.imp().columns.borrow().iter().enumerate().find(|(_, column)| column.as_ptr() as usize == current_column).map_or(0, |(i, _)| i as u32); // 0 corresponds to the name column

                    if SETTINGS.apps_sort_by() != current_column_number {
                        let _ = SETTINGS.set_apps_sort_by(current_column_number);
                    }

                    if SETTINGS.apps_sort_by_ascending() != sorter.primary_sort_order() {
                        let _ = SETTINGS.set_apps_sort_by_ascending(sorter.primary_sort_order());
                    }
                }
            }));
        }
    }

    pub fn open_information_dialog(&self, app_item: &AppItem) {
        let imp = self.imp();
        let app_dialog = ResAppDialog::new();
        app_dialog.init(app_item);
        app_dialog.present(&MainWindow::default());
        *imp.open_dialog.borrow_mut() = Some((
            app_item.id.as_ref().map(std::string::ToString::to_string),
            app_dialog,
        ));
    }

    fn search_filter(&self, obj: &Object) -> bool {
        let imp = self.imp();
        let item = obj.downcast_ref::<ApplicationEntry>().unwrap();
        let search_string = imp.search_entry.text().to_string().to_lowercase();
        !imp.search_revealer.reveals_child()
            || item.name().to_lowercase().contains(&search_string)
            || item
                .id()
                .is_some_and(|id| id.to_lowercase().contains(&search_string))
            || item
                .description()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&search_string)
    }

    pub fn get_selected_app_item(&self) -> Option<AppItem> {
        self.imp()
            .selection_model
            .borrow()
            .selected_item()
            .and_then(|object| object.downcast::<ApplicationEntry>().unwrap().app_item())
    }

    pub fn refresh_apps_list(&self, apps: &AppsContext) {
        let imp = self.imp();

        let store = imp.store.borrow_mut();
        let mut dialog_opt = &*imp.open_dialog.borrow_mut();

        let mut new_items = apps.app_items();
        let mut ids_to_remove = HashSet::new();

        // change process entries of apps that have run before
        store
            .iter::<ApplicationEntry>()
            .flatten()
            .for_each(|object| {
                let app_id = object.id().map(|gs| gs.to_string());
                // filter out apps that have run before but don't anymore
                if app_id.is_some() // don't try to filter out "System Processes"
                    && !apps
                        .get_app(&app_id.clone().unwrap_or_default())
                        .unwrap()
                        .is_running()
                {
                    if let Some((dialog_id, dialog)) = dialog_opt {
                        if dialog_id.as_deref() == app_id.as_deref() {
                            dialog.close();
                            dialog_opt = &None;
                        }
                    }
                    *imp.popped_over_app.borrow_mut() = None;
                    ids_to_remove.insert(app_id.clone());
                }
                if let Some((_, new_item)) = new_items.remove_entry(&app_id) {
                    if let Some((dialog_id, dialog)) = dialog_opt {
                        if *dialog_id == app_id {
                            dialog.update(&new_item);
                        }
                    }
                    object.update(new_item);
                }
            });

        // remove apps that recently have stopped running
        store.retain(|object| {
            !ids_to_remove.contains(
                &object
                    .clone()
                    .downcast::<ApplicationEntry>()
                    .unwrap()
                    .id()
                    .map(|gs| gs.to_string()),
            )
        });

        // add the newly started apps to the store
        let items: Vec<ApplicationEntry> = new_items
            .drain()
            .map(|(_, new_item)| ApplicationEntry::new(new_item))
            .collect();
        store.extend_from_slice(&items);

        if let Some(sorter) = imp.column_view.borrow().sorter() {
            sorter.changed(gtk::SorterChange::Different);
        }

        // -1 because we don't want to count System Processes
        self.set_property(
            "tab_usage_string",
            i18n_f("Running Apps: {}", &[&(store.n_items() - 1).to_string()]),
        );
    }

    pub fn execute_process_action_dialog(&self, app: AppItem, action: ProcessAction) {
        // Nothing too bad can happen on Continue so dont show the dialog
        if action == ProcessAction::CONT {
            let main_context = MainContext::default();
            main_context.spawn_local(clone!(@strong self as this => async move {
                let imp = this.imp();
                let _ = imp.sender.get().unwrap().send(
                    Action::ManipulateApp(action, app.id.unwrap(), imp.toast_overlay.get())
                ).await;
            }));
            return;
        }

        // Confirmation dialog & warning
        let dialog = adw::AlertDialog::builder()
            .heading(get_action_name(action, &[&app.display_name]))
            .body(get_app_action_warning(action))
            .build();

        dialog.add_response("yes", &get_app_action_description(action));
        dialog.set_response_appearance("yes", ResponseAppearance::Destructive);

        dialog.add_response("no", &i18n("Cancel"));
        dialog.set_default_response(Some("no"));
        dialog.set_close_response("no");

        // Called when "yes" or "no" were clicked
        dialog.connect_response(
            None,
            clone!(@strong self as this => move |_, response| {
                if response == "yes" {
                    let main_context = MainContext::default();
                    main_context.spawn_local(clone!(@strong this, @strong app => async move {
                        let imp = this.imp();
                        let _ = imp.sender.get().unwrap().send(
                            Action::ManipulateApp(action, app.id.clone().unwrap(), imp.toast_overlay.get())
                        ).await;
                    }));
                }
            }),
        );

        dialog.present(&MainWindow::default());
    }

    fn add_name_column(column_view: &ColumnView) -> ColumnViewColumn {
        let name_col_factory = gtk::SignalListItemFactory::new();

        let name_col =
            gtk::ColumnViewColumn::new(Some(&i18n("App")), Some(name_col_factory.clone()));

        name_col.set_resizable(true);

        name_col.set_expand(true);

        name_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = ResApplicationNameCell::new();

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("name")
                .bind(&row, "name", Widget::NONE);

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("icon")
                .bind(&row, "icon", Widget::NONE);

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("symbolic")
                .bind(&row, "symbolic", Widget::NONE);
        });

        let name_col_sorter = StringSorter::builder()
            .ignore_case(true)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "name",
            ))
            .build();

        name_col.set_sorter(Some(&name_col_sorter));

        column_view.append_column(&name_col);

        name_col
    }

    fn add_memory_column(column_view: &ColumnView) -> ColumnViewColumn {
        let memory_col_factory = gtk::SignalListItemFactory::new();

        let memory_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Memory")), Some(memory_col_factory.clone()));

        memory_col.set_resizable(true);

        memory_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);

            row.set_min_chars(9);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("memory_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, memory_usage: u64| {
                    convert_storage(memory_usage as f64, false)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let memory_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "memory_usage",
            ))
            .build();

        memory_col.set_sorter(Some(&memory_col_sorter));
        memory_col.set_visible(SETTINGS.apps_show_memory());

        column_view.append_column(&memory_col);

        SETTINGS.connect_apps_show_memory(
            clone!(@strong memory_col => move |visible| memory_col.set_visible(visible)),
        );

        memory_col
    }

    fn add_cpu_column(column_view: &ColumnView) -> ColumnViewColumn {
        let cpu_col_factory = gtk::SignalListItemFactory::new();

        let cpu_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Processor")), Some(cpu_col_factory.clone()));

        cpu_col.set_resizable(true);

        cpu_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(7);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("cpu_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, cpu_usage: f32| {
                    let mut percentage = cpu_usage * 100.0;
                    if !SETTINGS.normalize_cpu_usage() {
                        percentage *= *NUM_CPUS as f32;
                    }

                    format!("{percentage:.1} %")
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let cpu_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "cpu_usage",
            ))
            .build();

        cpu_col.set_sorter(Some(&cpu_col_sorter));
        cpu_col.set_visible(SETTINGS.apps_show_cpu());

        column_view.append_column(&cpu_col);

        SETTINGS.connect_apps_show_cpu(
            clone!(@strong cpu_col => move |visible| cpu_col.set_visible(visible)),
        );

        cpu_col
    }

    fn add_read_speed_column(column_view: &ColumnView) -> ColumnViewColumn {
        let read_speed_col_factory = gtk::SignalListItemFactory::new();

        let read_speed_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Drive Read")),
            Some(read_speed_col_factory.clone()),
        );

        read_speed_col.set_resizable(true);

        read_speed_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(11);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("read_speed")
                .chain_closure::<String>(closure!(|_: Option<Object>, read_speed: f64| {
                    if read_speed == -1.0 {
                        i18n("N/A")
                    } else {
                        convert_speed(read_speed, false)
                    }
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let read_speed_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "read_speed",
            ))
            .build();

        read_speed_col.set_sorter(Some(&read_speed_col_sorter));
        read_speed_col.set_visible(SETTINGS.apps_show_drive_read_speed());

        column_view.append_column(&read_speed_col);

        SETTINGS.connect_apps_show_drive_read_speed(
            clone!(@strong read_speed_col => move |visible| read_speed_col.set_visible(visible)),
        );

        read_speed_col
    }

    fn add_read_total_column(column_view: &ColumnView) -> ColumnViewColumn {
        let read_total_col_factory = gtk::SignalListItemFactory::new();

        let read_total_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Drive Read Total")),
            Some(read_total_col_factory.clone()),
        );

        read_total_col.set_resizable(true);

        read_total_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(9);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("read_total")
                .chain_closure::<String>(closure!(|_: Option<Object>, read_total: u64| {
                    convert_storage(read_total as f64, false)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let read_total_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "read_total",
            ))
            .build();

        read_total_col.set_sorter(Some(&read_total_col_sorter));
        read_total_col.set_visible(SETTINGS.apps_show_drive_read_total());

        column_view.append_column(&read_total_col);

        SETTINGS.connect_apps_show_drive_read_total(
            clone!(@strong read_total_col => move |visible| read_total_col.set_visible(visible)),
        );

        read_total_col
    }

    fn add_write_speed_column(column_view: &ColumnView) -> ColumnViewColumn {
        let write_speed_col_factory = gtk::SignalListItemFactory::new();

        let write_speed_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Drive Write")),
            Some(write_speed_col_factory.clone()),
        );

        write_speed_col.set_resizable(true);

        write_speed_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(11);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("write_speed")
                .chain_closure::<String>(closure!(|_: Option<Object>, write_speed: f64| {
                    if write_speed == -1.0 {
                        i18n("N/A")
                    } else {
                        convert_speed(write_speed, false)
                    }
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let write_speed_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "write_speed",
            ))
            .build();

        write_speed_col.set_sorter(Some(&write_speed_col_sorter));
        write_speed_col.set_visible(SETTINGS.apps_show_drive_write_speed());

        column_view.append_column(&write_speed_col);

        SETTINGS.connect_apps_show_drive_write_speed(
            clone!(@strong write_speed_col => move |visible| {
                write_speed_col.set_visible(visible);
            }),
        );

        write_speed_col
    }

    fn add_write_total_column(column_view: &ColumnView) -> ColumnViewColumn {
        let write_total_col_factory = gtk::SignalListItemFactory::new();

        let write_total_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Drive Write Total")),
            Some(write_total_col_factory.clone()),
        );

        write_total_col.set_resizable(true);

        write_total_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(9);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("write_total")
                .chain_closure::<String>(closure!(|_: Option<Object>, write_total: u64| {
                    convert_storage(write_total as f64, false)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let write_total_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "write_total",
            ))
            .build();

        write_total_col.set_sorter(Some(&write_total_col_sorter));
        write_total_col.set_visible(SETTINGS.apps_show_drive_write_total());

        column_view.append_column(&write_total_col);

        SETTINGS.connect_apps_show_drive_write_total(
            clone!(@strong write_total_col => move |visible| {
                write_total_col.set_visible(visible);
            }),
        );

        write_total_col
    }

    fn add_gpu_column(column_view: &ColumnView) -> ColumnViewColumn {
        let gpu_col_factory = gtk::SignalListItemFactory::new();

        let gpu_col = gtk::ColumnViewColumn::new(Some(&i18n("GPU")), Some(gpu_col_factory.clone()));

        gpu_col.set_resizable(true);

        gpu_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(7);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("gpu_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, gpu_usage: f32| {
                    format!("{:.1} %", gpu_usage * 100.0)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let gpu_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "gpu_usage",
            ))
            .build();

        gpu_col.set_sorter(Some(&gpu_col_sorter));
        gpu_col.set_visible(SETTINGS.apps_show_gpu());

        column_view.append_column(&gpu_col);

        SETTINGS.connect_apps_show_gpu(
            clone!(@strong gpu_col => move |visible| gpu_col.set_visible(visible)),
        );

        gpu_col
    }

    fn add_encoder_column(column_view: &ColumnView) -> ColumnViewColumn {
        let encoder_col_factory = gtk::SignalListItemFactory::new();

        let encoder_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Video Encoder")),
            Some(encoder_col_factory.clone()),
        );

        encoder_col.set_resizable(true);

        encoder_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(7);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("enc_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, enc_usage: f32| {
                    format!("{:.1} %", enc_usage * 100.0)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let encoder_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "enc_usage",
            ))
            .build();

        encoder_col.set_sorter(Some(&encoder_col_sorter));
        encoder_col.set_visible(SETTINGS.apps_show_encoder());

        column_view.append_column(&encoder_col);

        SETTINGS.connect_apps_show_encoder(
            clone!(@strong encoder_col => move |visible| encoder_col.set_visible(visible)),
        );

        encoder_col
    }

    fn add_decoder_column(column_view: &ColumnView) -> ColumnViewColumn {
        let decoder_col_factory = gtk::SignalListItemFactory::new();

        let decoder_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Video Decoder")),
            Some(decoder_col_factory.clone()),
        );

        decoder_col.set_resizable(true);

        decoder_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(7);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ApplicationEntry>("dec_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, dec_usage: f32| {
                    format!("{:.1} %", dec_usage * 100.0)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let decoder_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "dec_usage",
            ))
            .build();

        decoder_col.set_sorter(Some(&decoder_col_sorter));
        decoder_col.set_visible(SETTINGS.apps_show_decoder());

        column_view.append_column(&decoder_col);

        SETTINGS.connect_apps_show_decoder(
            clone!(@strong decoder_col => move |visible| decoder_col.set_visible(visible)),
        );

        decoder_col
    }

    fn add_gpu_mem_column(column_view: &ColumnView) -> ColumnViewColumn {
        let gpu_mem_col_factory = gtk::SignalListItemFactory::new();
        let gpu_mem_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Video Memory")),
            Some(gpu_mem_col_factory.clone()),
        );
        gpu_mem_col.set_resizable(true);

        gpu_mem_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(9);

            item.set_child(Some(&row));
            item.property_expression("item")
                .chain_property::<ApplicationEntry>("gpu_mem_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, gpu_mem: u64| {
                    convert_storage(gpu_mem as f64, false)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let gpu_mem_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ApplicationEntry::static_type(),
                None::<&gtk::Expression>,
                "gpu_mem_usage",
            ))
            .build();

        gpu_mem_col.set_sorter(Some(&gpu_mem_col_sorter));
        gpu_mem_col.set_visible(SETTINGS.apps_show_gpu_memory());

        column_view.append_column(&gpu_mem_col);

        SETTINGS.connect_apps_show_gpu_memory(
            clone!(@strong gpu_mem_col => move |visible| gpu_mem_col.set_visible(visible)),
        );

        gpu_mem_col
    }
}

fn get_action_name(action: ProcessAction, args: &[&str]) -> String {
    match action {
        ProcessAction::TERM => i18n_f("End {}?", args),
        ProcessAction::STOP => i18n_f("Halt {}?", args),
        ProcessAction::KILL => i18n_f("Kill {}?", args),
        ProcessAction::CONT => i18n_f("Continue {}?", args),
    }
}

fn get_app_action_warning(action: ProcessAction) -> String {
    match action {
            ProcessAction::TERM => i18n("Unsaved work might be lost."),
            ProcessAction::STOP => i18n("Halting an app can come with serious risks such as losing data and security implications. Use with caution."),
            ProcessAction::KILL => i18n("Killing an app can come with serious risks such as losing data and security implications. Use with caution."),
            ProcessAction::CONT => String::new(),
        }
}

fn get_app_action_description(action: ProcessAction) -> String {
    match action {
        ProcessAction::TERM => i18n("End App"),
        ProcessAction::STOP => i18n("Halt App"),
        ProcessAction::KILL => i18n("Kill App"),
        ProcessAction::CONT => i18n("Continue App"),
    }
}
