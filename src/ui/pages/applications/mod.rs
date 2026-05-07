pub mod application_entry;
mod application_name_cell;

use std::collections::HashSet;

use adw::ResponseAppearance;
use adw::{prelude::*, subclass::prelude::*};
use async_channel::Sender;
use gtk::accessible::Property;
use gtk::glib::{self, MainContext, Object, clone, closure};
use gtk::{
    ColumnView, ColumnViewColumn, EventControllerKey, FilterChange, ListItem, NumericSorter,
    SortType, StringSorter, Widget, gio,
};

use crate::add_column;
use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::ui::dialogs::app_dialog::ResAppDialog;
use crate::ui::pages::{MAX_PERCENTAGE_LENGTH, MAX_SPEED_LENGTH, MAX_STORAGE_LENGTH};
use crate::ui::window::{Action, MainWindow};
use crate::utils::NUM_CPUS;
use crate::utils::app::AppsContext;
use crate::utils::process::ProcessAction;
use crate::utils::settings::SETTINGS;
use crate::utils::units::{convert_fraction, convert_speed, convert_storage};

use self::application_entry::ApplicationEntry;
use self::application_name_cell::ResApplicationNameCell;

pub const TAB_ID: &str = "applications";

mod imp {
    use std::{
        cell::{Cell, RefCell},
        sync::OnceLock,
    };

    use crate::ui::{pages::APPLICATIONS_PRIMARY_ORD, window::Action};

    use super::*;

    use gtk::{
        ColumnViewColumn, CompositeTemplate,
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
    };

    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/applications.ui")]
    #[properties(wrapper_type = super::ResApplications)]
    pub struct ResApplications {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub popover_menu: TemplateChild<gtk::PopoverMenu>,
        #[template_child]
        pub search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub applications_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub information_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub end_application_button: TemplateChild<adw::SplitButton>,
        #[template_child]
        pub toolbar_view: TemplateChild<adw::ToolbarView>,

        pub store: RefCell<gio::ListStore>,
        pub selection_model: RefCell<gtk::SingleSelection>,
        pub filter_model: RefCell<gtk::FilterListModel>,
        pub sort_model: RefCell<gtk::SortListModel>,
        pub column_view: RefCell<gtk::ColumnView>,
        pub open_info_dialog: RefCell<Option<(Option<String>, ResAppDialog)>>,
        pub info_dialog_closed: Cell<bool>,

        pub sender: OnceLock<Sender<Action>>,

        pub popped_over_app: RefCell<Option<ApplicationEntry>>,

        pub columns: RefCell<Vec<ColumnViewColumn>>,

        #[property(get)]
        uses_progress_bar: Cell<bool>,

        #[property(get)]
        icon: RefCell<Icon>,

        #[property(get = Self::tab_name, type = glib::GString)]
        tab_name: Cell<glib::GString>,

        #[property(get = Self::tab_detail_string, type = glib::GString)]
        tab_detail_string: Cell<glib::GString>,

        #[property(get = Self::tab_usage_string, set = Self::set_tab_usage_string, type = glib::GString)]
        tab_usage_string: Cell<glib::GString>,

        #[property(get = Self::tab_id, type = glib::GString)]
        tab_id: Cell<glib::GString>,

        #[property(get)]
        graph_locked_max_y: Cell<bool>,

        #[property(get)]
        primary_ord: Cell<u32>,

        #[property(get)]
        secondary_ord: Cell<u32>,
    }

    impl ResApplications {
        gstring_getter_setter!(tab_name, tab_detail_string, tab_usage_string, tab_id);
    }

    impl Default for ResApplications {
        fn default() -> Self {
            Self {
                toast_overlay: Default::default(),
                popover_menu: Default::default(),
                search_bar: Default::default(),
                search_entry: Default::default(),
                information_button: Default::default(),
                store: gio::ListStore::new::<ApplicationEntry>().into(),
                selection_model: Default::default(),
                filter_model: Default::default(),
                sort_model: Default::default(),
                column_view: Default::default(),
                open_info_dialog: Default::default(),
                info_dialog_closed: Default::default(),
                sender: Default::default(),
                applications_scrolled_window: Default::default(),
                end_application_button: Default::default(),
                toolbar_view: Default::default(),
                uses_progress_bar: Cell::new(false),
                icon: RefCell::new(ThemedIcon::new("app-symbolic").into()),
                tab_name: Cell::from(glib::GString::from(i18n("Apps"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_usage_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::from(TAB_ID)),
                popped_over_app: Default::default(),
                columns: Default::default(),
                graph_locked_max_y: Cell::new(true),
                primary_ord: Cell::new(APPLICATIONS_PRIMARY_ORD),
                secondary_ord: Default::default(),
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
                "applications.context-end-app",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        res_applications
                            .open_app_action_dialog(application_entry, ProcessAction::TERM);
                    }
                },
            );

            klass.install_action(
                "applications.context-kill-app",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        res_applications
                            .open_app_action_dialog(application_entry, ProcessAction::KILL);
                    }
                },
            );

            klass.install_action(
                "applications.context-halt-app",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        res_applications
                            .open_app_action_dialog(application_entry, ProcessAction::STOP);
                    }
                },
            );

            klass.install_action(
                "applications.context-continue-app",
                None,
                move |res_applications, _, _| {
                    if let Some(application_entry) =
                        res_applications.imp().popped_over_app.borrow().as_ref()
                    {
                        res_applications
                            .open_app_action_dialog(application_entry, ProcessAction::CONT);
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
                        res_applications.open_info_dialog(application_entry);
                    }
                },
            );

            klass.install_action(
                "applications.kill-app",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_entry() {
                        res_applications.open_app_action_dialog(&app, ProcessAction::KILL);
                    }
                },
            );

            klass.install_action(
                "applications.halt-app",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_entry() {
                        res_applications.open_app_action_dialog(&app, ProcessAction::STOP);
                    }
                },
            );

            klass.install_action(
                "applications.continue-app",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_entry() {
                        res_applications.open_app_action_dialog(&app, ProcessAction::CONT);
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
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Accessible;
}

impl Default for ResApplications {
    fn default() -> Self {
        Self::new()
    }
}

impl ResApplications {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn toggle_search(&self) {
        let imp = self.imp();
        imp.search_bar
            .set_search_mode(!imp.search_bar.is_search_mode());
    }

    pub fn close_search(&self) {
        let imp = self.imp();
        imp.search_bar.set_search_mode(false);
    }

    pub fn init(&self, sender: Sender<Action>) {
        let imp = self.imp();
        imp.sender.set(sender).unwrap();

        self.setup_widgets();
        self.setup_signals();
    }

    fn add_gestures(&self, item: &ListItem) {
        let widget = item.child().unwrap();

        let secondary_click = gtk::GestureClick::new();
        secondary_click.set_button(3);
        secondary_click.connect_released(clone!(
            #[weak]
            widget,
            #[weak]
            item,
            #[weak(rename_to = this)]
            self,
            move |_, _, x, y| {
                if let Some(entry) = item.item().and_downcast::<ApplicationEntry>() {
                    let imp = this.imp();
                    let popover_menu = &imp.popover_menu;

                    *imp.popped_over_app.borrow_mut() = Some(entry);

                    let position = widget
                        .compute_point(&this, &gtk::graphene::Point::new(x as _, y as _))
                        .unwrap();

                    popover_menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                        position.x().round() as i32,
                        position.y().round() as i32,
                        0,
                        0,
                    )));

                    popover_menu.popup();
                }
            }
        ));

        widget.add_controller(secondary_click);
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();

        imp.popover_menu.set_parent(self);

        *imp.column_view.borrow_mut() = gtk::ColumnView::new(None::<gtk::SingleSelection>);

        self.setup_columns();

        let column_view = imp.column_view.borrow();
        let columns = imp.columns.borrow();

        column_view.set_tab_behavior(gtk::ListTabBehavior::Cell);

        let store = gio::ListStore::new::<ApplicationEntry>();

        let filter_model = gtk::FilterListModel::new(
            Some(store.clone()),
            Some(gtk::CustomFilter::new(clone!(
                #[strong(rename_to = this)]
                self,
                move |obj| this.search_filter(obj)
            ))),
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

    fn setup_columns(&self) {
        let imp = self.imp();
        let column_view = imp.column_view.borrow();
        let mut columns = imp.columns.borrow_mut();

        columns.push(self.add_name_column(&column_view));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Memory"),
            property: memory_usage,
            value_type: u64,
            min_chars: MAX_STORAGE_LENGTH,
            sorter: numeric,
            convert: |v: u64| convert_storage(v as f64, false),
            settings_show: apps_show_memory,
            settings_connect: connect_apps_show_memory,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Processor"),
            property: cpu_usage,
            value_type: f32,
            min_chars: MAX_PERCENTAGE_LENGTH,
            sorter: numeric,
            convert: |v: f32| {
                let mut fraction = v;
                if !SETTINGS.normalize_cpu_usage() {
                    fraction *= *NUM_CPUS as f32;
                }
                convert_fraction(fraction as f64, false)
            },
            settings_show: apps_show_cpu,
            settings_connect: connect_apps_show_cpu,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Drive Read"),
            property: read_speed,
            value_type: f64,
            min_chars: MAX_SPEED_LENGTH,
            sorter: numeric,
            convert: |v: f64| if v == -1.0 { i18n("N/A") } else { convert_speed(v, false) },
            settings_show: apps_show_drive_read_speed,
            settings_connect: connect_apps_show_drive_read_speed,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Drive Read Total"),
            property: read_total,
            value_type: u64,
            min_chars: MAX_SPEED_LENGTH,
            sorter: numeric,
            convert: |v: u64| convert_storage(v as f64, false),
            settings_show: apps_show_drive_read_total,
            settings_connect: connect_apps_show_drive_read_total,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Drive Write"),
            property: write_speed,
            value_type: f64,
            min_chars: MAX_STORAGE_LENGTH,
            sorter: numeric,
            convert: |v: f64| if v == -1.0 { i18n("N/A") } else { convert_speed(v, false) },
            settings_show: apps_show_drive_write_speed,
            settings_connect: connect_apps_show_drive_write_speed,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Drive Write Total"),
            property: write_total,
            value_type: u64,
            min_chars: MAX_STORAGE_LENGTH,
            sorter: numeric,
            convert: |v: u64| convert_storage(v as f64, false),
            settings_show: apps_show_drive_write_total,
            settings_connect: connect_apps_show_drive_write_total,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("GPU"),
            property: gpu_usage,
            value_type: f32,
            min_chars: MAX_PERCENTAGE_LENGTH,
            sorter: numeric,
            convert: |v: f32| convert_fraction(v as f64, false),
            settings_show: apps_show_gpu,
            settings_connect: connect_apps_show_gpu,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Video Memory"),
            property: gpu_mem_usage,
            value_type: u64,
            min_chars: MAX_STORAGE_LENGTH,
            sorter: numeric,
            convert: |v: u64| convert_storage(v as f64, false),
            settings_show: apps_show_gpu_memory,
            settings_connect: connect_apps_show_gpu_memory,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Video Encoder"),
            property: enc_usage,
            value_type: f32,
            min_chars: MAX_PERCENTAGE_LENGTH,
            sorter: numeric,
            convert: |v: f32| convert_fraction(v as f64, false),
            settings_show: apps_show_encoder,
            settings_connect: connect_apps_show_encoder,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Video Decoder"),
            property: dec_usage,
            value_type: f32,
            min_chars: MAX_PERCENTAGE_LENGTH,
            sorter: numeric,
            convert: |v: f32| convert_fraction(v as f64, false),
            settings_show: apps_show_decoder,
            settings_connect: connect_apps_show_decoder,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Swap"),
            property: swap_usage,
            value_type: u64,
            min_chars: MAX_STORAGE_LENGTH,
            sorter: numeric,
            convert: |v: u64| convert_storage(v as f64, false),
            settings_show: apps_show_swap,
            settings_connect: connect_apps_show_swap,
        ));

        columns.push(add_column!(
            this: self,
            column_view: &column_view,
            entry_type: ApplicationEntry,
            title: i18n("Combined Memory"),
            property: combined_memory_usage,
            value_type: u64,
            min_chars: MAX_STORAGE_LENGTH,
            sorter: numeric,
            convert: |v: u64| convert_storage(v as f64, false),
            settings_show: apps_show_combined_memory,
            settings_connect: connect_apps_show_combined_memory,
        ));
    }

    pub fn setup_signals(&self) {
        let imp = self.imp();

        imp.selection_model
            .borrow()
            .connect_selection_changed(clone!(
                #[weak]
                imp,
                move |model, _, _| {
                    imp.toolbar_view
                        .set_reveal_bottom_bars(model.selected_item().is_some());

                    let is_system_processes = model.selected_item().is_some_and(|object| {
                        object
                            .downcast::<ApplicationEntry>()
                            .unwrap()
                            .id()
                            .is_none()
                    });

                    imp.information_button
                        .set_sensitive(model.selected() != u32::MAX);
                    imp.end_application_button
                        .set_sensitive(model.selected() != u32::MAX && !is_system_processes);
                }
            ));

        imp.search_bar
            .set_key_capture_widget(self.parent().as_ref());

        imp.search_entry.connect_search_changed(clone!(
            #[weak]
            imp,
            move |_| {
                if let Some(filter) = imp.filter_model.borrow().filter() {
                    filter.changed(FilterChange::Different);
                }
            }
        ));

        let event_controller = EventControllerKey::new();
        event_controller.connect_key_released(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, key, _, _| {
                if key.name().unwrap_or_default() == "Escape" {
                    this.close_search();
                }
            }
        ));
        imp.search_bar.add_controller(event_controller);

        imp.information_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let imp = this.imp();
                let selection_option = imp
                    .selection_model
                    .borrow()
                    .selected_item()
                    .map(|object| object.downcast::<ApplicationEntry>().unwrap());
                if let Some(selection) = selection_option {
                    this.open_info_dialog(&selection);
                }
            }
        ));

        imp.end_application_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                if let Some(app) = this.get_selected_app_entry() {
                    this.open_app_action_dialog(&app, ProcessAction::TERM);
                }
            }
        ));

        if let Some(column_view_sorter) = imp.column_view.borrow().sorter() {
            column_view_sorter.connect_changed(clone!(
                #[weak]
                imp,
                move |sorter, _| {
                    if let Some(sorter) = sorter.downcast_ref::<gtk::ColumnViewSorter>() {
                        let current_column = sorter
                            .primary_sort_column()
                            .map(|column| column.as_ptr() as usize)
                            .unwrap_or_default();

                        let current_column_number = imp
                            .columns
                            .borrow()
                            .iter()
                            .enumerate()
                            .find(|(_, column)| column.as_ptr() as usize == current_column)
                            .map_or(0, |(i, _)| i as u32); // 0 corresponds to the name column

                        if SETTINGS.apps_sort_by() != current_column_number {
                            let _ = SETTINGS.set_apps_sort_by(current_column_number);
                        }

                        if SETTINGS.apps_sort_by_ascending() != sorter.primary_sort_order() {
                            let _ =
                                SETTINGS.set_apps_sort_by_ascending(sorter.primary_sort_order());
                        }
                    }
                }
            ));
        }
    }

    pub fn search_bar(&self) -> &gtk::SearchBar {
        &self.imp().search_bar
    }

    pub fn open_info_dialog(&self, app: &ApplicationEntry) {
        let imp = self.imp();

        if imp.open_info_dialog.borrow().is_some() {
            return;
        }

        imp.info_dialog_closed.set(false);

        let dialog = ResAppDialog::new();

        dialog.init(app);

        AdwDialogExt::present(&dialog, Some(&MainWindow::default()));

        dialog.connect_closed(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.imp().info_dialog_closed.set(true);
            }
        ));

        *imp.open_info_dialog.borrow_mut() = Some((
            app.id().as_ref().map(std::string::ToString::to_string),
            dialog,
        ));
    }

    fn search_filter(&self, obj: &Object) -> bool {
        let imp = self.imp();
        let item = obj.downcast_ref::<ApplicationEntry>().unwrap();
        let search_string = imp.search_entry.text().to_string().to_lowercase();
        !imp.search_bar.is_search_mode()
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

    pub fn get_selected_app_entry(&self) -> Option<ApplicationEntry> {
        self.imp()
            .selection_model
            .borrow()
            .selected_item()
            .and_then(|object| object.downcast::<ApplicationEntry>().ok())
    }

    pub fn refresh_apps_list(&self, apps_context: &AppsContext) {
        let imp = self.imp();

        if imp.info_dialog_closed.get() {
            let _ = imp.open_info_dialog.take();
            imp.info_dialog_closed.set(false);
        }

        let store = imp.store.borrow_mut();
        let mut dialog_opt = &*imp.open_info_dialog.borrow_mut();

        let mut ids_to_remove = HashSet::new();
        let mut already_existing_ids = HashSet::new();

        // change process entries of apps that have run before
        store
            .iter::<ApplicationEntry>()
            .flatten()
            .for_each(|object| {
                let app_id = object.id().map(|gs| gs.to_string());
                // filter out apps that have run before but don't anymore
                if app_id.is_some() // don't try to filter out "System Processes"
                    && !apps_context
                        .get_app(&app_id)
                        .unwrap()
                        .is_running()
                {
                    if let Some((dialog_id, dialog)) = dialog_opt {
                        if dialog_id.as_deref() == app_id.as_deref() {
                            AdwDialogExt::close(dialog);
                            dialog_opt = &None;
                        }
                    }
                    *imp.popped_over_app.borrow_mut() = None;
                    ids_to_remove.insert(app_id.clone());
                }

                if let Some(app) = apps_context.get_app(&app_id) {
                    object.update(app, apps_context);
                    if let Some((dialog_id, dialog)) = dialog_opt {
                        if *dialog_id == app_id {
                            dialog.update(&object);
                        }
                    }
                    already_existing_ids.insert(app_id);
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
        let items: Vec<ApplicationEntry> = apps_context
            .running_apps_iter()
            .filter(|app| {
                !already_existing_ids.contains(&app.id) && !ids_to_remove.contains(&app.id)
            })
            .map(|new_item| ApplicationEntry::new(new_item, apps_context))
            .collect();
        store.extend_from_slice(&items);

        if let Some(sorter) = imp.column_view.borrow().sorter() {
            sorter.changed(gtk::SorterChange::Different);
        }

        if imp.selection_model.borrow().selection().size() == 0 {
            imp.toolbar_view.set_reveal_bottom_bars(false);
            imp.information_button.set_sensitive(false);
            imp.end_application_button.set_sensitive(false);
        }

        // -1 because we don't want to count System Processes
        self.set_tab_usage_string(i18n_f(
            "Running Apps: {}",
            &[&(store.n_items().saturating_sub(1)).to_string()],
        ));
    }

    pub fn open_app_action_dialog(&self, app: &ApplicationEntry, action: ProcessAction) {
        // Nothing too bad can happen on Continue so dont show the dialog
        if action == ProcessAction::CONT {
            let main_context = MainContext::default();
            main_context.spawn_local(clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                app,
                async move {
                    let imp = this.imp();
                    let _ = imp
                        .sender
                        .get()
                        .unwrap()
                        .send(Action::ManipulateApp(
                            action,
                            app.id().unwrap().to_string(),
                            imp.toast_overlay.get(),
                        ))
                        .await;
                }
            ));
            return;
        }

        // Confirmation dialog & warning
        let dialog = adw::AlertDialog::builder()
            .heading(get_action_name(action, &app.name()))
            .body(get_action_warning(action))
            .build();

        dialog.add_response("yes", &get_action_description(action));
        dialog.set_response_appearance("yes", ResponseAppearance::Destructive);

        dialog.add_response("no", &i18n("Cancel"));
        dialog.set_default_response(Some("no"));
        dialog.set_close_response("no");

        // Called when "yes" or "no" were clicked
        dialog.connect_response(
            None,
            clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                app,
                move |_, response| {
                    if response == "yes" {
                        let main_context = MainContext::default();
                        main_context.spawn_local(clone!(
                            #[weak]
                            this,
                            #[strong]
                            app,
                            async move {
                                let imp = this.imp();
                                let _ = imp
                                    .sender
                                    .get()
                                    .unwrap()
                                    .send(Action::ManipulateApp(
                                        action,
                                        app.id().unwrap().to_string(),
                                        imp.toast_overlay.get(),
                                    ))
                                    .await;
                            }
                        ));
                    }
                }
            ),
        );

        dialog.present(Some(&MainWindow::default()));
    }

    fn add_name_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let name_col_factory = gtk::SignalListItemFactory::new();

        let name_col =
            gtk::ColumnViewColumn::new(Some(&i18n("App")), Some(name_col_factory.clone()));

        name_col.set_resizable(true);

        name_col.set_expand(true);

        name_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
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

                this.add_gestures(item);
            }
        ));

        name_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&ResApplicationNameCell>);
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
}

fn get_action_name(action: ProcessAction, name: &str) -> String {
    match action {
        ProcessAction::TERM => i18n_f("End {}?", &[name]),
        ProcessAction::STOP => i18n_f("Halt {}?", &[name]),
        ProcessAction::KILL => i18n_f("Kill {}?", &[name]),
        ProcessAction::CONT => i18n_f("Continue {}?", &[name]),
    }
}

fn get_action_warning(action: ProcessAction) -> String {
    match action {
        ProcessAction::TERM => i18n("Unsaved work might be lost"),
        ProcessAction::STOP => i18n(
            "Halting an app can come with serious risks such as losing data and security implications. Use with caution.",
        ),
        ProcessAction::KILL => i18n(
            "Killing an app can come with serious risks such as losing data and security implications. Use with caution.",
        ),
        ProcessAction::CONT => String::new(),
    }
}

fn get_action_description(action: ProcessAction) -> String {
    match action {
        ProcessAction::TERM => i18n("End App"),
        ProcessAction::STOP => i18n("Halt App"),
        ProcessAction::KILL => i18n("Kill App"),
        ProcessAction::CONT => i18n("Continue App"),
    }
}
