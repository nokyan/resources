mod process_entry;
mod process_name_cell;

use std::collections::HashSet;

use adw::ResponseAppearance;
use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, closure, Object, Sender};
use gtk::{
    gio, ColumnView, ColumnViewColumn, FilterChange, NumericSorter, SortType, StringSorter, Widget,
};
use gtk_macros::send;

use log::error;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::ui::dialogs::process_dialog::ResProcessDialog;
use crate::ui::window::{self, Action, MainWindow};
use crate::utils::app::AppsContext;
use crate::utils::process::{ProcessAction, ProcessItem};
use crate::utils::settings::SETTINGS;
use crate::utils::units::{convert_speed, convert_storage};

use self::process_entry::ProcessEntry;
use self::process_name_cell::ResProcessNameCell;

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
        sync::OnceLock,
    };

    use crate::{ui::window::Action, utils::process::ProcessAction};

    use super::*;

    use gtk::{
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Sender, Value},
        CompositeTemplate,
    };

    #[derive(CompositeTemplate, Properties)]
    #[properties(wrapper_type = super::ResProcesses)]
    #[template(resource = "/net/nokyan/Resources/ui/pages/processes.ui")]
    pub struct ResProcesses {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub popover_menu: TemplateChild<gtk::PopoverMenu>,
        #[template_child]
        pub search_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub processes_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub search_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub information_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub end_process_button: TemplateChild<adw::SplitButton>,

        pub store: RefCell<gio::ListStore>,
        pub selection_model: RefCell<gtk::SingleSelection>,
        pub filter_model: RefCell<gtk::FilterListModel>,
        pub sort_model: RefCell<gtk::SortListModel>,
        pub column_view: RefCell<gtk::ColumnView>,
        pub open_dialog: RefCell<Option<(i32, ResProcessDialog)>>,

        pub username_cache: RefCell<HashMap<u32, String>>,

        pub sender: OnceLock<Sender<Action>>,

        pub popped_over_process: RefCell<Option<ProcessEntry>>,

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
    }

    impl ResProcesses {
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

        pub fn set_tab_detail(&self, detail: &str) {
            self.tab_detail_string.set(glib::GString::from(detail));
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
    }

    impl Default for ResProcesses {
        fn default() -> Self {
            Self {
                toast_overlay: Default::default(),
                search_revealer: Default::default(),
                search_entry: Default::default(),
                processes_scrolled_window: Default::default(),
                search_button: Default::default(),
                information_button: Default::default(),
                end_process_button: Default::default(),
                store: gio::ListStore::new::<ProcessEntry>().into(),
                selection_model: Default::default(),
                filter_model: Default::default(),
                sort_model: Default::default(),
                column_view: Default::default(),
                open_dialog: Default::default(),
                username_cache: Default::default(),
                sender: Default::default(),
                uses_progress_bar: Cell::new(false),
                icon: RefCell::new(ThemedIcon::new("generic-process-symbolic").into()),
                tab_name: Cell::new(glib::GString::from(i18n("Processes"))),
                tab_detail_string: Cell::new(glib::GString::from("")),
                tab_usage_string: Cell::new(glib::GString::from("")),
                popover_menu: Default::default(),
                popped_over_process: Default::default(),
                columns: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResProcesses {
        const NAME: &'static str = "ResProcesses";
        type Type = super::ResProcesses;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action(
                "processes.context-end-process",
                None,
                move |res_processes, _, _| {
                    if let Some(process_entry) =
                        res_processes.imp().popped_over_process.borrow().as_ref()
                    {
                        if let Some(process_item) = process_entry.process_item() {
                            res_processes
                                .execute_process_action_dialog(process_item, ProcessAction::TERM);
                        }
                    }
                },
            );

            klass.install_action(
                "processes.context-kill-process",
                None,
                move |res_processes, _, _| {
                    if let Some(process_entry) =
                        res_processes.imp().popped_over_process.borrow().as_ref()
                    {
                        if let Some(process_item) = process_entry.process_item() {
                            res_processes
                                .execute_process_action_dialog(process_item, ProcessAction::KILL);
                        }
                    }
                },
            );

            klass.install_action(
                "processes.context-halt-process",
                None,
                move |res_processes, _, _| {
                    if let Some(process_entry) =
                        res_processes.imp().popped_over_process.borrow().as_ref()
                    {
                        if let Some(process_item) = process_entry.process_item() {
                            res_processes
                                .execute_process_action_dialog(process_item, ProcessAction::STOP);
                        }
                    }
                },
            );

            klass.install_action(
                "processes.context-continue-process",
                None,
                move |res_processes, _, _| {
                    if let Some(process_entry) =
                        res_processes.imp().popped_over_process.borrow().as_ref()
                    {
                        if let Some(process_item) = process_entry.process_item() {
                            res_processes
                                .execute_process_action_dialog(process_item, ProcessAction::CONT);
                        }
                    }
                },
            );

            klass.install_action(
                "processes.context-information",
                None,
                move |res_processes, _, _| {
                    if let Some(process_entry) =
                        res_processes.imp().popped_over_process.borrow().as_ref()
                    {
                        res_processes
                            .open_information_dialog(&process_entry.process_item().unwrap());
                    }
                },
            );

            klass.install_action(
                "processes.kill-process",
                None,
                move |res_processes, _, _| {
                    if let Some(process) = res_processes.get_selected_process_item() {
                        res_processes.execute_process_action_dialog(process, ProcessAction::KILL);
                    }
                },
            );

            klass.install_action(
                "processes.halt-process",
                None,
                move |res_processes, _, _| {
                    if let Some(process) = res_processes.get_selected_process_item() {
                        res_processes.execute_process_action_dialog(process, ProcessAction::STOP);
                    }
                },
            );

            klass.install_action(
                "processes.continue-process",
                None,
                move |res_processes, _, _| {
                    if let Some(process) = res_processes.get_selected_process_item() {
                        res_processes.execute_process_action_dialog(process, ProcessAction::CONT);
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

    impl ObjectImpl for ResProcesses {
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

    impl WidgetImpl for ResProcesses {}
    impl BinImpl for ResProcesses {}
}

glib::wrapper! {
    pub struct ResProcesses(ObjectSubclass<imp::ResProcesses>)
        @extends gtk::Widget, adw::Bin;
}

impl ResProcesses {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn toggle_search(&self) {
        let imp = self.imp();

        imp.search_button.set_active(!imp.search_button.is_active());
    }

    pub fn init(&self, sender: Sender<Action>) {
        let imp = self.imp();
        imp.sender.set(sender).unwrap();

        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();

        imp.popover_menu.set_parent(self);

        *imp.column_view.borrow_mut() = gtk::ColumnView::new(None::<gtk::SingleSelection>);
        let column_view = imp.column_view.borrow();

        let mut columns = imp.columns.borrow_mut();

        columns.push(self.add_name_column(&column_view));
        columns.push(self.add_pid_column(&column_view));
        columns.push(self.add_user_column(&column_view));
        columns.push(self.add_memory_column(&column_view));
        columns.push(self.add_cpu_column(&column_view));
        columns.push(self.add_read_speed_column(&column_view));
        columns.push(self.add_read_total_column(&column_view));
        columns.push(self.add_write_speed_column(&column_view));
        columns.push(self.add_write_total_column(&column_view));
        columns.push(self.add_gpu_column(&column_view));
        columns.push(self.add_gpu_mem_column(&column_view));
        columns.push(self.add_encoder_column(&column_view));
        columns.push(self.add_decoder_column(&column_view));

        let store = gio::ListStore::new::<ProcessEntry>();

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
                .get(SETTINGS.processes_sort_by() as usize)
                .or_else(|| columns.get(3)),
            SETTINGS.processes_sort_by_ascending(),
        );

        column_view.add_css_class("resources-columnview");

        *imp.store.borrow_mut() = store;
        *imp.selection_model.borrow_mut() = selection_model;
        *imp.sort_model.borrow_mut() = sort_model;
        *imp.filter_model.borrow_mut() = filter_model;

        imp.processes_scrolled_window.set_child(Some(&*column_view));
    }

    pub fn setup_signals(&self) {
        let imp = self.imp();

        imp.selection_model.borrow().connect_selection_changed(
            clone!(@strong self as this => move |model, _, _| {
                let imp = this.imp();
                imp.information_button.set_sensitive(model.selected() != u32::MAX);
                imp.end_process_button.set_sensitive(model.selected() != u32::MAX);
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

        imp.information_button
            .connect_clicked(clone!(@strong self as this => move |_| {
            let imp = this.imp();
                let selection_option = imp.selection_model.borrow()
                .selected_item()
                .map(|object| {
                    object
                    .downcast::<ProcessEntry>()
                    .unwrap()
                });
                if let Some(selection) = selection_option {
                    this.open_information_dialog(&selection.process_item().unwrap());
                }
            }));

        imp.end_process_button
            .connect_clicked(clone!(@strong self as this => move |_| {
                if let Some(app) = this.get_selected_process_item() {
                    this.execute_process_action_dialog(app, ProcessAction::TERM);
                }
            }));

        if let Some(column_view_sorter) = imp.column_view.borrow().sorter() {
            column_view_sorter.connect_changed(clone!(@strong self as this => move |sorter, _| {
                if let Some(sorter) = sorter.downcast_ref::<gtk::ColumnViewSorter>() {
                    let current_column = sorter.primary_sort_column().map(|column| column.as_ptr() as usize).unwrap_or_default();

                    let current_column_number = this.imp().columns.borrow().iter().enumerate().find(|(_, column)| column.as_ptr() as usize == current_column).map(|(i, _)| i as u32).unwrap_or(3); // 3 corresponds to the memory column

                    if SETTINGS.processes_sort_by() != current_column_number {
                        let _ = SETTINGS.set_processes_sort_by(current_column_number);
                    }

                    if SETTINGS.processes_sort_by_ascending() != sorter.primary_sort_order() {
                        let _ = SETTINGS.set_processes_sort_by_ascending(sorter.primary_sort_order());
                    }
                }
            }));
        }
    }

    pub fn open_information_dialog(&self, process: &ProcessItem) {
        let imp = self.imp();
        let process_dialog = ResProcessDialog::new();
        process_dialog.init(process, self.get_user_name_by_uid(process.uid));
        process_dialog.set_visible(true);
        *imp.open_dialog.borrow_mut() = Some((process.pid, process_dialog));
    }

    fn search_filter(&self, obj: &Object) -> bool {
        let imp = self.imp();
        let item = obj.downcast_ref::<ProcessEntry>().unwrap();
        let search_string = imp.search_entry.text().to_string().to_lowercase();
        !imp.search_revealer.reveals_child()
            || item.name().to_lowercase().contains(&search_string)
            || item.commandline().to_lowercase().contains(&search_string)
    }

    pub fn get_selected_process_item(&self) -> Option<ProcessItem> {
        self.imp()
            .selection_model
            .borrow()
            .selected_item()
            .and_then(|object| object.downcast::<ProcessEntry>().unwrap().process_item())
    }

    pub fn refresh_processes_list(&self, apps: &AppsContext) {
        let imp = self.imp();

        let store = imp.store.borrow_mut();
        let mut dialog_opt = &*imp.open_dialog.borrow_mut();

        let mut new_items = apps.process_items();
        let mut pids_to_remove = HashSet::new();

        // change process entries of processes that have existed before
        store.iter::<ProcessEntry>().flatten().for_each(|object| {
            let item_pid = object.pid();
            // filter out processes that have existed before but don't anymore
            if apps.get_process(item_pid).is_none() {
                if let Some((dialog_pid, dialog)) = dialog_opt {
                    if *dialog_pid == item_pid {
                        dialog.close();
                        dialog_opt = &None;
                    }
                }
                imp.popover_menu.popdown();
                *imp.popped_over_process.borrow_mut() = None;
                pids_to_remove.insert(item_pid);
            }
            if let Some((_, new_item)) = new_items.remove_entry(&item_pid) {
                if let Some((dialog_pid, dialog)) = dialog_opt {
                    if *dialog_pid == item_pid {
                        dialog.update(&new_item);
                    }
                }
                object.update(new_item);
            }
        });

        // remove recently deceased processes
        store.retain(|object| {
            !pids_to_remove.contains(&object.clone().downcast::<ProcessEntry>().unwrap().pid())
        });

        // add the newly started process to the store
        let items: Vec<ProcessEntry> = new_items
            .drain()
            .map(|(_, new_item)| {
                let user_name = self.get_user_name_by_uid(new_item.uid);
                ProcessEntry::new(new_item, &user_name)
            })
            .collect();
        store.extend_from_slice(&items);

        if let Some(sorter) = imp.column_view.borrow().sorter() {
            sorter.changed(gtk::SorterChange::Different)
        }

        self.set_property(
            "tab_usage_string",
            i18n_f("Running Processes: {}", &[&(store.n_items()).to_string()]),
        );
    }

    pub fn execute_process_action_dialog(&self, process: ProcessItem, action: ProcessAction) {
        let imp = self.imp();

        // Nothing too bad can happen on Continue so dont show the dialog
        if action == ProcessAction::CONT {
            send!(
                imp.sender.get().unwrap(),
                Action::ManipulateProcess(
                    action,
                    process.pid,
                    process.display_name,
                    imp.toast_overlay.get()
                )
            );
            return;
        }

        // Confirmation dialog & warning
        let dialog = adw::MessageDialog::builder()
            .transient_for(&MainWindow::default())
            .modal(true)
            .heading(window::get_action_name(action, &[&process.display_name]))
            .body(window::get_app_action_warning(action))
            .build();

        dialog.add_response("yes", &window::get_app_action_description(action));
        dialog.set_response_appearance("yes", ResponseAppearance::Destructive);

        dialog.add_response("no", &i18n("Cancel"));
        dialog.set_default_response(Some("no"));
        dialog.set_close_response("no");

        // Called when "yes" or "no" were clicked
        dialog.connect_response(
            None,
            clone!(@strong self as this, @strong process => move |_, response| {
                if response == "yes" {
                    let imp = this.imp();
                    send!(
                        imp.sender.get().unwrap(),
                        Action::ManipulateProcess(
                            action,
                            process.pid,
                            process.clone().display_name,
                            imp.toast_overlay.get()
                        )
                    );
                }
            }),
        );

        dialog.set_visible(true);
    }

    fn get_user_name_by_uid(&self, uid: u32) -> String {
        let imp = self.imp();

        // we do this to avoid mut-borrows when possible
        let cached = {
            let borrow = imp.username_cache.borrow();
            borrow.get(&uid).cloned()
        };

        if let Some(cached) = cached {
            cached
        } else {
            let name = uzers::get_user_by_uid(uid).map_or_else(
                || i18n("root"),
                |user| user.name().to_string_lossy().to_string(),
            );
            imp.username_cache.borrow_mut().insert(uid, name.clone());
            name
        }
    }

    fn add_name_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let name_col_factory = gtk::SignalListItemFactory::new();

        let name_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Process")), Some(name_col_factory.clone()));

        name_col.set_resizable(true);

        name_col.set_expand(true);

        name_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = ResProcessNameCell::new();

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ProcessEntry>("name")
                .bind(&row, "name", Widget::NONE);

            item.property_expression("item")
                .chain_property::<ProcessEntry>("icon")
                .bind(&row, "icon", Widget::NONE);

            item.property_expression("item")
                .chain_property::<ProcessEntry>("commandline")
                .bind(&row, "tooltip", Widget::NONE);
        });

        let name_col_sorter = StringSorter::builder()
            .ignore_case(true)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "name",
            ))
            .build();

        name_col.set_sorter(Some(&name_col_sorter));

        column_view.append_column(&name_col);

        name_col
    }

    fn add_pid_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let pid_col_factory = gtk::SignalListItemFactory::new();

        let pid_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Process ID")), Some(pid_col_factory.clone()));

        pid_col.set_resizable(true);

        pid_col_factory.connect_setup(clone!(@strong self as this => move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);

            item.set_child(Some(&row));
            item.property_expression("item")
                .chain_property::<ProcessEntry>("pid")
                .bind(&row, "text", Widget::NONE);
        }));

        let pid_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "pid",
            ))
            .build();

        pid_col.set_sorter(Some(&pid_col_sorter));
        pid_col.set_visible(SETTINGS.processes_show_id());

        column_view.append_column(&pid_col);

        SETTINGS.connect_processes_show_id(
            clone!(@strong pid_col => move |visible| pid_col.set_visible(visible)),
        );

        pid_col
    }

    fn add_user_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let user_col_factory = gtk::SignalListItemFactory::new();

        let user_col =
            gtk::ColumnViewColumn::new(Some(&i18n("User")), Some(user_col_factory.clone()));

        user_col.set_resizable(true);

        user_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ProcessEntry>("user")
                .bind(&row, "text", Widget::NONE);
        });

        let user_col_sorter = StringSorter::builder()
            .ignore_case(true)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "user",
            ))
            .build();

        user_col.set_sorter(Some(&user_col_sorter));
        user_col.set_visible(SETTINGS.processes_show_user());

        column_view.append_column(&user_col);

        SETTINGS.connect_processes_show_user(
            clone!(@strong user_col => move |visible| user_col.set_visible(visible)),
        );

        user_col
    }

    fn add_memory_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("memory_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, memory_usage: u64| {
                    convert_storage(memory_usage as f64, false)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let memory_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "memory_usage",
            ))
            .build();

        memory_col.set_sorter(Some(&memory_col_sorter));
        memory_col.set_visible(SETTINGS.processes_show_memory());

        column_view.append_column(&memory_col);

        SETTINGS.connect_processes_show_memory(
            clone!(@strong memory_col => move |visible| memory_col.set_visible(visible)),
        );

        memory_col
    }

    fn add_cpu_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("cpu_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, cpu_usage: f32| {
                    format!("{:.1} %", cpu_usage * 100.0)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let cpu_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "cpu_usage",
            ))
            .build();

        cpu_col.set_sorter(Some(&cpu_col_sorter));
        cpu_col.set_visible(SETTINGS.processes_show_cpu());

        column_view.append_column(&cpu_col);

        SETTINGS.connect_processes_show_cpu(
            clone!(@strong cpu_col => move |visible| cpu_col.set_visible(visible)),
        );

        cpu_col
    }

    fn add_read_speed_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("read_speed")
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
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "read_speed",
            ))
            .build();

        read_speed_col.set_sorter(Some(&read_speed_col_sorter));
        read_speed_col.set_visible(SETTINGS.processes_show_drive_read_speed());

        column_view.append_column(&read_speed_col);

        SETTINGS.connect_processes_show_drive_read_speed(
            clone!(@strong read_speed_col => move  |visible| {
                read_speed_col.set_visible(visible)
            }),
        );

        read_speed_col
    }

    fn add_read_total_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("read_total")
                .chain_closure::<String>(closure!(|_: Option<Object>, read_total: i64| {
                    if read_total == -1 {
                        i18n("N/A")
                    } else {
                        convert_storage(read_total as f64, false)
                    }
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let read_total_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "read_total",
            ))
            .build();

        read_total_col.set_sorter(Some(&read_total_col_sorter));
        read_total_col.set_visible(SETTINGS.processes_show_drive_read_total());

        column_view.append_column(&read_total_col);

        SETTINGS.connect_processes_show_drive_read_total(
            clone!(@strong read_total_col => move |visible| {
                read_total_col.set_visible(visible)
            }),
        );

        read_total_col
    }

    fn add_write_speed_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("write_speed")
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
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "write_speed",
            ))
            .build();

        write_speed_col.set_sorter(Some(&write_speed_col_sorter));
        write_speed_col.set_visible(SETTINGS.processes_show_drive_write_speed());

        column_view.append_column(&write_speed_col);

        SETTINGS.connect_processes_show_drive_write_speed(
            clone!(@strong write_speed_col => move |visible| {
                write_speed_col.set_visible(visible)
            }),
        );

        write_speed_col
    }

    fn add_write_total_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("write_total")
                .chain_closure::<String>(closure!(|_: Option<Object>, write_total: i64| {
                    if write_total == -1 {
                        i18n("N/A")
                    } else {
                        convert_storage(write_total as f64, false)
                    }
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let write_total_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "write_total",
            ))
            .build();

        write_total_col.set_sorter(Some(&write_total_col_sorter));
        write_total_col.set_visible(SETTINGS.processes_show_drive_write_total());

        column_view.append_column(&write_total_col);

        SETTINGS.connect_processes_show_drive_write_total(
            clone!(@strong write_total_col => move  |visible| {
                write_total_col.set_visible(visible)
            }),
        );

        write_total_col
    }

    fn add_gpu_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let gpu_col_factory = gtk::SignalListItemFactory::new();

        let gpu_col = gtk::ColumnViewColumn::new(Some(&i18n("GPU")), Some(gpu_col_factory.clone()));

        gpu_col.set_resizable(true);

        gpu_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();

            let row = gtk::Inscription::new(None);
            row.set_min_chars(7);

            item.set_child(Some(&row));

            item.property_expression("item")
                .chain_property::<ProcessEntry>("gpu_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, gpu_usage: f32| {
                    format!("{:.1} %", gpu_usage * 100.0)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let gpu_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "gpu_usage",
            ))
            .build();

        gpu_col.set_sorter(Some(&gpu_col_sorter));
        gpu_col.set_visible(SETTINGS.processes_show_gpu());

        column_view.append_column(&gpu_col);

        SETTINGS.connect_processes_show_gpu(
            clone!(@strong gpu_col => move  |visible| gpu_col.set_visible(visible)),
        );

        gpu_col
    }

    fn add_encoder_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("enc_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, enc_usage: f32| {
                    format!("{:.1} %", enc_usage * 100.0)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let encoder_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "enc_usage",
            ))
            .build();

        encoder_col.set_sorter(Some(&encoder_col_sorter));
        encoder_col.set_visible(SETTINGS.processes_show_encoder());

        column_view.append_column(&encoder_col);

        SETTINGS.connect_processes_show_encoder(
            clone!(@strong encoder_col => move  |visible| encoder_col.set_visible(visible)),
        );

        encoder_col
    }

    fn add_decoder_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("dec_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, dec_usage: f32| {
                    format!("{:.1} %", dec_usage * 100.0)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let decoder_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "dec_usage",
            ))
            .build();

        decoder_col.set_sorter(Some(&decoder_col_sorter));
        decoder_col.set_visible(SETTINGS.processes_show_decoder());

        column_view.append_column(&decoder_col);

        SETTINGS.connect_processes_show_decoder(
            clone!(@strong decoder_col => move  |visible| decoder_col.set_visible(visible)),
        );

        decoder_col
    }

    fn add_gpu_mem_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
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
                .chain_property::<ProcessEntry>("gpu_mem_usage")
                .chain_closure::<String>(closure!(|_: Option<Object>, gpu_mem: u64| {
                    convert_storage(gpu_mem as f64, false)
                }))
                .bind(&row, "text", Widget::NONE);
        });

        let gpu_mem_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "gpu_mem_usage",
            ))
            .build();

        gpu_mem_col.set_sorter(Some(&gpu_mem_col_sorter));
        gpu_mem_col.set_visible(SETTINGS.processes_show_gpu_memory());

        column_view.append_column(&gpu_mem_col);

        SETTINGS.connect_processes_show_gpu_memory(
            clone!(@strong gpu_mem_col => move  |visible| gpu_mem_col.set_visible(visible)),
        );

        gpu_mem_col
    }
}
