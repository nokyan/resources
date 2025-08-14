pub mod process_entry;
mod process_name_cell;

use std::collections::HashSet;
use std::sync::LazyLock;

use adw::ResponseAppearance;
use adw::{prelude::*, subclass::prelude::*};
use async_channel::Sender;
use gtk::glib::{self, MainContext, Object, clone, closure};
use gtk::{
    BitsetIter, ColumnView, ColumnViewColumn, EventControllerKey, FilterChange, ListItem,
    NumericSorter, SortType, StringSorter, Widget, gio,
};
use process_data::Niceness;

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f, ni18n_f};
use crate::ui::dialogs::process_dialog::ResProcessDialog;
use crate::ui::dialogs::process_options_dialog::ResProcessOptionsDialog;
use crate::ui::pages::NICE_TO_LABEL;
use crate::ui::window::{Action, MainWindow};
use crate::utils::NUM_CPUS;
use crate::utils::app::AppsContext;
use crate::utils::process::ProcessAction;
use crate::utils::settings::SETTINGS;
use crate::utils::units::{convert_speed, convert_storage, format_time};

use self::process_entry::ProcessEntry;
use self::process_name_cell::ResProcessNameCell;

pub const TAB_ID: &str = "processes";

static LONGEST_PRIORITY_LABEL: LazyLock<u32> = LazyLock::new(|| {
    // make sure that no matter how short the longest current locale's translation for a priority may be, a signed
    // two-digit number (+ 1 for more space) will always fit
    let min_length = 4;
    let calulated = NICE_TO_LABEL
        .values()
        .map(|(s, _)| s.len())
        .max()
        .unwrap_or(13) as u32;

    if calulated > min_length {
        calulated
    } else {
        min_length
    }
});

mod imp {
    use std::{
        cell::{Cell, RefCell},
        sync::OnceLock,
    };

    use crate::{
        ui::{
            dialogs::process_options_dialog::ResProcessOptionsDialog, pages::PROCESSES_PRIMARY_ORD,
            window::Action,
        },
        utils::process::ProcessAction,
    };

    use super::*;

    use gtk::{
        CompositeTemplate,
        gio::{Icon, ThemedIcon},
        glib::{ParamSpec, Properties, Value},
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
        pub popover_menu_multiple: TemplateChild<gtk::PopoverMenu>,
        #[template_child]
        pub search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub processes_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub kill_window_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub logout_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub reboot_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub shutdown_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub options_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub information_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub end_process_button: TemplateChild<adw::SplitButton>,
        #[template_child]
        pub end_process_menu: TemplateChild<gio::MenuModel>,
        #[template_child]
        pub end_process_menu_multiple: TemplateChild<gio::MenuModel>,
        pub store: RefCell<gio::ListStore>,
        pub selection_model: RefCell<gtk::MultiSelection>,
        pub filter_model: RefCell<gtk::FilterListModel>,
        pub sort_model: RefCell<gtk::SortListModel>,
        pub column_view: RefCell<gtk::ColumnView>,

        pub open_info_dialog: RefCell<Option<(i32, ResProcessDialog)>>,
        pub open_options_dialog: RefCell<Option<(i32, ResProcessOptionsDialog)>>,

        pub info_dialog_closed: Cell<bool>,
        pub options_dialog_closed: Cell<bool>,

        pub sender: OnceLock<Sender<Action>>,

        pub popped_over_process: RefCell<Option<ProcessEntry>>,

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

    impl ResProcesses {
        gstring_getter_setter!(tab_name, tab_detail_string, tab_usage_string, tab_id);
    }

    impl Default for ResProcesses {
        fn default() -> Self {
            Self {
                toast_overlay: Default::default(),
                popover_menu: Default::default(),
                popover_menu_multiple: Default::default(),
                search_bar: Default::default(),
                search_entry: Default::default(),
                processes_scrolled_window: Default::default(),
                kill_window_button: Default::default(),
                logout_button: Default::default(),
                reboot_button: Default::default(),
                shutdown_button: Default::default(),
                options_button: Default::default(),
                information_button: Default::default(),
                end_process_button: Default::default(),
                end_process_menu: Default::default(),
                end_process_menu_multiple: Default::default(),
                store: gio::ListStore::new::<ProcessEntry>().into(),
                selection_model: RefCell::new(glib::object::Object::new::<gtk::MultiSelection>()),
                filter_model: Default::default(),
                sort_model: Default::default(),
                column_view: Default::default(),
                open_info_dialog: Default::default(),
                open_options_dialog: Default::default(),
                info_dialog_closed: Default::default(),
                options_dialog_closed: Default::default(),
                sender: Default::default(),
                uses_progress_bar: Cell::new(false),
                icon: RefCell::new(ThemedIcon::new("generic-process-symbolic").into()),
                tab_name: Cell::new(glib::GString::from(i18n("Processes"))),
                tab_detail_string: Cell::new(glib::GString::new()),
                tab_usage_string: Cell::new(glib::GString::new()),
                tab_id: Cell::new(glib::GString::from(TAB_ID)),
                popped_over_process: Default::default(),
                columns: Default::default(),
                graph_locked_max_y: Cell::new(true),
                primary_ord: Cell::new(PROCESSES_PRIMARY_ORD),
                secondary_ord: Default::default(),
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
                        res_processes.open_process_action_dialog(
                            vec![process_entry.clone()],
                            ProcessAction::TERM,
                        );
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
                        res_processes.open_process_action_dialog(
                            vec![process_entry.clone()],
                            ProcessAction::KILL,
                        );
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
                        res_processes.open_process_action_dialog(
                            vec![process_entry.clone()],
                            ProcessAction::STOP,
                        );
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
                        res_processes.open_process_action_dialog(
                            vec![process_entry.clone()],
                            ProcessAction::CONT,
                        );
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
                        res_processes.open_info_dialog(process_entry);
                    }
                },
            );

            klass.install_action(
                "processes.context-options",
                None,
                move |res_processes, _, _| {
                    if let Some(process_entry) =
                        res_processes.imp().popped_over_process.borrow().as_ref()
                    {
                        res_processes.open_options_dialog(process_entry);
                    }
                },
            );

            klass.install_action("processes.end-process", None, move |res_processes, _, _| {
                let selected = res_processes.get_selected_process_entries();
                if !selected.is_empty() {
                    res_processes.open_process_action_dialog(selected, ProcessAction::TERM);
                }
            });

            klass.install_action(
                "processes.kill-process",
                None,
                move |res_processes, _, _| {
                    let selected = res_processes.get_selected_process_entries();
                    if !selected.is_empty() {
                        res_processes.open_process_action_dialog(selected, ProcessAction::KILL);
                    }
                },
            );

            klass.install_action(
                "processes.halt-process",
                None,
                move |res_processes, _, _| {
                    let selected = res_processes.get_selected_process_entries();
                    if !selected.is_empty() {
                        res_processes.open_process_action_dialog(selected, ProcessAction::STOP);
                    }
                },
            );

            klass.install_action(
                "processes.continue-process",
                None,
                move |res_processes, _, _| {
                    let selected = res_processes.get_selected_process_entries();
                    if !selected.is_empty() {
                        res_processes.open_process_action_dialog(selected, ProcessAction::CONT);
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

impl Default for ResProcesses {
    fn default() -> Self {
        Self::new()
    }
}

impl ResProcesses {
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
                if let Some(entry) = item.item().and_downcast::<ProcessEntry>() {
                    let imp = this.imp();

                    let selected = this.get_selected_process_entries();

                    let popover_menu = if selected.len() > 1 {
                        &imp.popover_menu_multiple
                    } else {
                        &imp.popover_menu
                    };

                    *imp.popped_over_process.borrow_mut() = Some(entry);

                    let position = widget
                        .compute_point(&this, &gtk::graphene::Point::new(x as _, y as _))
                        .unwrap();

                    popover_menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                        position.x().round() as i32,
                        position.y().round() as i32,
                        1,
                        1,
                    )));

                    popover_menu.popup();
                }
            }
        ));

        widget.add_controller(secondary_click);
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();

        // i don't quite get why that's necessary
        imp.popover_menu.set_parent(self);
        imp.popover_menu_multiple.set_parent(self);

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
        columns.push(self.add_total_cpu_time_column(&column_view));
        columns.push(self.add_user_cpu_time_column(&column_view));
        columns.push(self.add_system_cpu_time_column(&column_view));
        columns.push(self.add_priority_column(&column_view));
        columns.push(self.add_swap_column(&column_view));
        columns.push(self.add_combined_memory_column(&column_view));

        let store = gio::ListStore::new::<ProcessEntry>();

        let filter_model = gtk::FilterListModel::new(
            Some(store.clone()),
            Some(gtk::CustomFilter::new(clone!(
                #[strong(rename_to = this)]
                self,
                move |obj| this.search_filter(obj)
            ))),
        );

        let sort_model = gtk::SortListModel::new(Some(filter_model.clone()), column_view.sorter());

        let selection_model = gtk::MultiSelection::new(Some(sort_model.clone()));

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

        imp.end_process_button
            .set_menu_model(Some(&imp.end_process_menu.get()));

        imp.selection_model
            .borrow()
            .connect_selection_changed(clone!(
                #[weak(rename_to = this)]
                self,
                move |model, _, _| {
                    let imp = this.imp();
                    let bitset = model.selection();

                    imp.information_button.set_sensitive(bitset.size() == 1);
                    imp.options_button.set_sensitive(bitset.size() == 1);
                    imp.end_process_button.set_sensitive(bitset.size() > 0);

                    if bitset.size() <= 1 {
                        imp.end_process_button.set_label(&i18n("End Process"));
                        imp.end_process_button
                            .set_menu_model(Some(&imp.end_process_menu.get()));
                    } else {
                        imp.end_process_button.set_label(&i18n("End Processes"));
                        imp.end_process_button
                            .set_menu_model(Some(&imp.end_process_menu_multiple.get()));
                    }
                }
            ));

        imp.search_bar
            .set_key_capture_widget(self.parent().as_ref());

        imp.search_entry.connect_search_changed(clone!(
            #[strong(rename_to = this)]
            self,
            move |_| {
                let imp = this.imp();
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
        imp.search_entry.add_controller(event_controller);

        imp.options_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let imp = this.imp();
                let bitset = imp.selection_model.borrow().selection();
                let selection_option = imp
                    .selection_model
                    .borrow()
                    .item(bitset.maximum()) // the info button is only available when only 1 item is selected, so this should be fine
                    .map(|object| object.downcast::<ProcessEntry>().unwrap());
                if let Some(selection) = selection_option {
                    this.open_options_dialog(&selection);
                }
            }
        ));

        imp.information_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let imp = this.imp();
                let bitset = imp.selection_model.borrow().selection();
                let selection_option = imp
                    .selection_model
                    .borrow()
                    .item(bitset.maximum()) // the info button is only available when only 1 item is selected, so this should be fine
                    .map(|object| object.downcast::<ProcessEntry>().unwrap());
                if let Some(selection) = selection_option {
                    this.open_info_dialog(&selection);
                }
            }
        ));

        imp.end_process_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let selected = this.get_selected_process_entries();
                if !selected.is_empty() {
                    this.open_process_action_dialog(selected, ProcessAction::TERM);
                }
            }
        ));

        if let Some(column_view_sorter) = imp.column_view.borrow().sorter() {
            column_view_sorter.connect_changed(clone!(
                #[weak(rename_to = this)]
                self,
                move |sorter, _| {
                    if let Some(sorter) = sorter.downcast_ref::<gtk::ColumnViewSorter>() {
                        let current_column = sorter
                            .primary_sort_column()
                            .map(|column| column.as_ptr() as usize)
                            .unwrap_or_default();

                        let current_column_number = this
                            .imp()
                            .columns
                            .borrow()
                            .iter()
                            .enumerate()
                            .find(|(_, column)| column.as_ptr() as usize == current_column)
                            .map_or(3, |(i, _)| i as u32); // 3 corresponds to the memory column

                        if SETTINGS.processes_sort_by() != current_column_number {
                            let _ = SETTINGS.set_processes_sort_by(current_column_number);
                        }

                        if SETTINGS.processes_sort_by_ascending() != sorter.primary_sort_order() {
                            let _ = SETTINGS
                                .set_processes_sort_by_ascending(sorter.primary_sort_order());
                        }
                    }
                }
            ));
        }
    }

    pub fn search_bar(&self) -> &gtk::SearchBar {
        &self.imp().search_bar
    }

    pub fn open_options_dialog(&self, process: &ProcessEntry) {
        let imp = self.imp();

        if imp.open_info_dialog.borrow().is_some() || imp.open_options_dialog.borrow().is_some() {
            return;
        }

        imp.options_dialog_closed.set(false);

        let dialog = ResProcessOptionsDialog::new();

        dialog.init(
            process,
            imp.sender.get().unwrap().clone(),
            &imp.toast_overlay,
        );

        dialog.connect_closed(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.imp().options_dialog_closed.set(true);
            }
        ));

        dialog.present(Some(&MainWindow::default()));

        *imp.open_options_dialog.borrow_mut() = Some((process.pid(), dialog));
    }

    pub fn open_info_dialog(&self, process: &ProcessEntry) {
        let imp = self.imp();

        if imp.open_info_dialog.borrow().is_some() || imp.open_options_dialog.borrow().is_some() {
            return;
        }

        imp.options_dialog_closed.set(false);

        let dialog = ResProcessDialog::new();

        dialog.init(process, process.user());

        dialog.connect_closed(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.imp().info_dialog_closed.set(true);
            }
        ));

        dialog.present(Some(&MainWindow::default()));

        *imp.open_info_dialog.borrow_mut() = Some((process.pid(), dialog));
    }

    fn search_filter(&self, obj: &Object) -> bool {
        let imp = self.imp();
        let item = obj.downcast_ref::<ProcessEntry>().unwrap();
        let search_string = imp.search_entry.text().to_string().to_lowercase();
        !imp.search_bar.is_search_mode()
            || item.name().to_lowercase().contains(&search_string)
            || item.commandline().to_lowercase().contains(&search_string)
    }

    pub fn get_selected_process_entries(&self) -> Vec<ProcessEntry> {
        let imp = self.imp();

        if let Some((bitset_iter, first)) =
            BitsetIter::init_first(&imp.selection_model.borrow().selection())
        {
            let mut return_vec: Vec<_> = bitset_iter
                .filter_map(|position| {
                    imp.selection_model
                        .borrow()
                        .item(position)
                        .map(|object| object.downcast::<ProcessEntry>().unwrap())
                })
                .collect();

            if let Some(first_process) = imp
                .selection_model
                .borrow()
                .item(first)
                .map(|object| object.downcast::<ProcessEntry>().unwrap())
            {
                return_vec.insert(0, first_process);
            }

            return_vec
        } else {
            Vec::default()
        }
    }

    pub fn refresh_processes_list(&self, apps_context: &AppsContext, kwin_running: bool) {
        let imp = self.imp();

        // Update button visibility based on settings
        imp.kill_window_button
            .set_visible(kwin_running && SETTINGS.show_kill_window_button());
        imp.logout_button.set_visible(SETTINGS.show_logout_button());
        imp.reboot_button.set_visible(SETTINGS.show_reboot_button());
        imp.shutdown_button
            .set_visible(SETTINGS.show_shutdown_button());

        if imp.info_dialog_closed.get() {
            let _ = imp.open_info_dialog.take();
            imp.info_dialog_closed.set(false);
        }

        if imp.options_dialog_closed.get() {
            let _ = imp.open_options_dialog.take();
            imp.options_dialog_closed.set(false);
        }

        let store = imp.store.borrow_mut();
        let mut info_dialog_opt = imp.open_info_dialog.borrow_mut();
        let mut options_dialog_opt = imp.open_options_dialog.borrow_mut();

        let mut pids_to_remove = HashSet::new();
        let mut already_existing_pids = HashSet::new();

        // change process entries of processes that have existed before
        store.iter::<ProcessEntry>().flatten().for_each(|object| {
            let item_pid = object.pid();
            if let Some(process) = apps_context.get_process(item_pid) {
                object.update(process);
                if let Some((dialog_pid, dialog)) = &*info_dialog_opt {
                    if *dialog_pid == item_pid {
                        dialog.update(&object);
                    }
                }
                already_existing_pids.insert(item_pid);
            } else {
                // filter out processes that have existed before but don't anymore
                if let Some((dialog_pid, dialog)) = &*info_dialog_opt {
                    if *dialog_pid == item_pid {
                        dialog.close();
                        *info_dialog_opt = None;
                    }
                }
                if let Some((dialog_pid, dialog)) = &*options_dialog_opt {
                    if *dialog_pid == item_pid {
                        dialog.close();
                        *options_dialog_opt = None;
                    }
                }
                *imp.popped_over_process.borrow_mut() = None;
                imp.popover_menu.set_visible(false);
                pids_to_remove.insert(item_pid);
            }
        });

        std::mem::drop(info_dialog_opt);
        std::mem::drop(options_dialog_opt);

        // remove recently deceased processes
        store.retain(|object| {
            !pids_to_remove.contains(&object.clone().downcast::<ProcessEntry>().unwrap().pid())
        });

        // add the newly started process to the store
        let items: Vec<ProcessEntry> = apps_context
            .processes_iter()
            .filter(|process| {
                !already_existing_pids.contains(&process.data.pid)
                    && !pids_to_remove.contains(&process.data.pid)
            })
            .map(ProcessEntry::new)
            .collect();
        store.extend_from_slice(&items);

        if let Some(sorter) = imp.column_view.borrow().sorter() {
            sorter.changed(gtk::SorterChange::Different);
        }

        self.set_tab_usage_string(i18n_f(
            "Running Processes: {}",
            &[&(store.n_items()).to_string()],
        ));
    }

    pub fn open_process_action_dialog(&self, processes: Vec<ProcessEntry>, action: ProcessAction) {
        // Nothing too bad can happen on Continue so dont show the dialog
        if action == ProcessAction::CONT {
            let main_context = MainContext::default();
            main_context.spawn_local(clone!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                processes,
                async move {
                    let imp = this.imp();
                    let _ = imp
                        .sender
                        .get()
                        .unwrap()
                        .send(Action::ManipulateProcesses(
                            action,
                            processes
                                .iter()
                                .map(process_entry::ProcessEntry::pid)
                                .collect(),
                            imp.toast_overlay.get(),
                        ))
                        .await;
                }
            ));
            return;
        }

        let action_name = if processes.len() == 1 {
            get_action_name(action, &processes[0].name())
        } else {
            get_action_name_multiple(action, processes.len())
        };

        // Confirmation dialog & warning
        let dialog = adw::AlertDialog::builder()
            .heading(action_name)
            .body(get_action_warning(action))
            .build();

        dialog.add_response("yes", &get_action_description(action));
        dialog.set_response_appearance("yes", ResponseAppearance::Destructive);

        dialog.add_response("no", &i18n("Cancel"));
        dialog.set_default_response(Some("no"));
        dialog.set_close_response("no");

        // wtf is this
        dialog.connect_response(
            None,
            clone!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                processes,
                move |_, response| {
                    if response == "yes" {
                        let main_context = MainContext::default();
                        main_context.spawn_local(clone!(
                            #[weak]
                            this,
                            #[strong]
                            processes,
                            async move {
                                let imp = this.imp();
                                let _ = imp
                                    .sender
                                    .get()
                                    .unwrap()
                                    .send(Action::ManipulateProcesses(
                                        action,
                                        processes
                                            .iter()
                                            .map(process_entry::ProcessEntry::pid)
                                            .collect(),
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
            gtk::ColumnViewColumn::new(Some(&i18n("Process")), Some(name_col_factory.clone()));

        name_col.set_resizable(true);

        name_col.set_expand(true);

        name_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
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

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("symbolic")
                    .bind(&row, "symbolic", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        name_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&ResProcessNameCell>);
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

        pid_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);

                item.set_child(Some(&row));
                item.property_expression("item")
                    .chain_property::<ProcessEntry>("pid")
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        pid_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
        });

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

        SETTINGS.connect_processes_show_id(clone!(
            #[weak]
            pid_col,
            move |visible| pid_col.set_visible(visible)
        ));

        pid_col
    }

    fn add_user_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let user_col_factory = gtk::SignalListItemFactory::new();

        let user_col =
            gtk::ColumnViewColumn::new(Some(&i18n("User")), Some(user_col_factory.clone()));

        user_col.set_resizable(true);

        user_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);

                item.set_child(Some(&row));

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("user")
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        user_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_user(clone!(
            #[weak]
            user_col,
            move |visible| user_col.set_visible(visible)
        ));

        user_col
    }

    fn add_memory_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let memory_col_factory = gtk::SignalListItemFactory::new();

        let memory_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Memory")), Some(memory_col_factory.clone()));

        memory_col.set_resizable(true);

        memory_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);
                row.set_xalign(1.0);

                item.set_child(Some(&row));

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("memory_usage")
                    .chain_closure::<String>(closure!(|_: Option<Object>, memory_usage: u64| {
                        convert_storage(memory_usage as f64, false)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        memory_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_memory(clone!(
            #[weak]
            memory_col,
            move |visible| memory_col.set_visible(visible)
        ));

        memory_col
    }

    fn add_cpu_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let cpu_col_factory = gtk::SignalListItemFactory::new();

        let cpu_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Processor")), Some(cpu_col_factory.clone()));

        cpu_col.set_resizable(true);

        cpu_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(7);
                row.set_xalign(1.0);

                item.set_child(Some(&row));

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("cpu_usage")
                    .chain_closure::<String>(closure!(|_: Option<Object>, cpu_usage: f32| {
                        let mut percentage = cpu_usage * 100.0;
                        if !SETTINGS.normalize_cpu_usage() {
                            percentage *= *NUM_CPUS as f32;
                        }

                        format!("{percentage:.1} %")
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        cpu_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_cpu(clone!(
            #[weak]
            cpu_col,
            move |visible| cpu_col.set_visible(visible)
        ));

        cpu_col
    }

    fn add_read_speed_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let read_speed_col_factory = gtk::SignalListItemFactory::new();

        let read_speed_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Drive Read")),
            Some(read_speed_col_factory.clone()),
        );

        read_speed_col.set_resizable(true);

        read_speed_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(11);
                row.set_xalign(1.0);

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

                this.add_gestures(item);
            }
        ));

        read_speed_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_drive_read_speed(clone!(
            #[weak]
            read_speed_col,
            move |visible| {
                read_speed_col.set_visible(visible);
            }
        ));

        read_speed_col
    }

    fn add_read_total_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let read_total_col_factory = gtk::SignalListItemFactory::new();

        let read_total_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Drive Read Total")),
            Some(read_total_col_factory.clone()),
        );

        read_total_col.set_resizable(true);

        read_total_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);
                row.set_xalign(1.0);

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

                this.add_gestures(item);
            }
        ));

        read_total_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_drive_read_total(clone!(
            #[weak]
            read_total_col,
            move |visible| {
                read_total_col.set_visible(visible);
            }
        ));

        read_total_col
    }

    fn add_write_speed_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let write_speed_col_factory = gtk::SignalListItemFactory::new();

        let write_speed_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Drive Write")),
            Some(write_speed_col_factory.clone()),
        );

        write_speed_col.set_resizable(true);

        write_speed_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(11);
                row.set_xalign(1.0);

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

                this.add_gestures(item);
            }
        ));

        write_speed_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_drive_write_speed(clone!(
            #[weak]
            write_speed_col,
            move |visible| {
                write_speed_col.set_visible(visible);
            }
        ));

        write_speed_col
    }

    fn add_write_total_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let write_total_col_factory = gtk::SignalListItemFactory::new();

        let write_total_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Drive Write Total")),
            Some(write_total_col_factory.clone()),
        );

        write_total_col.set_resizable(true);

        write_total_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);
                row.set_xalign(1.0);

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

                this.add_gestures(item);
            }
        ));

        write_total_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_drive_write_total(clone!(
            #[weak]
            write_total_col,
            move |visible| {
                write_total_col.set_visible(visible);
            }
        ));

        write_total_col
    }

    fn add_gpu_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let gpu_col_factory = gtk::SignalListItemFactory::new();

        let gpu_col = gtk::ColumnViewColumn::new(Some(&i18n("GPU")), Some(gpu_col_factory.clone()));

        gpu_col.set_resizable(true);

        gpu_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(7);
                row.set_xalign(1.0);

                item.set_child(Some(&row));

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("gpu_usage")
                    .chain_closure::<String>(closure!(|_: Option<Object>, gpu_usage: f32| {
                        format!("{:.1} %", gpu_usage * 100.0)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        gpu_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_gpu(clone!(
            #[weak]
            gpu_col,
            move |visible| gpu_col.set_visible(visible)
        ));

        gpu_col
    }

    fn add_encoder_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let encoder_col_factory = gtk::SignalListItemFactory::new();

        let encoder_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Video Encoder")),
            Some(encoder_col_factory.clone()),
        );

        encoder_col.set_resizable(true);

        encoder_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(7);
                row.set_xalign(1.0);

                item.set_child(Some(&row));

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("enc_usage")
                    .chain_closure::<String>(closure!(|_: Option<Object>, enc_usage: f32| {
                        format!("{:.1} %", enc_usage * 100.0)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        encoder_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_encoder(clone!(
            #[weak]
            encoder_col,
            move |visible| encoder_col.set_visible(visible)
        ));

        encoder_col
    }

    fn add_decoder_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let decoder_col_factory = gtk::SignalListItemFactory::new();

        let decoder_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Video Decoder")),
            Some(decoder_col_factory.clone()),
        );

        decoder_col.set_resizable(true);

        decoder_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(7);
                row.set_xalign(1.0);

                item.set_child(Some(&row));

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("dec_usage")
                    .chain_closure::<String>(closure!(|_: Option<Object>, dec_usage: f32| {
                        format!("{:.1} %", dec_usage * 100.0)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        decoder_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_decoder(clone!(
            #[weak]
            decoder_col,
            move |visible| decoder_col.set_visible(visible)
        ));

        decoder_col
    }

    fn add_gpu_mem_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let gpu_mem_col_factory = gtk::SignalListItemFactory::new();

        let gpu_mem_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Video Memory")),
            Some(gpu_mem_col_factory.clone()),
        );

        gpu_mem_col.set_resizable(true);

        gpu_mem_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);
                row.set_xalign(1.0);

                item.set_child(Some(&row));
                item.property_expression("item")
                    .chain_property::<ProcessEntry>("gpu_mem_usage")
                    .chain_closure::<String>(closure!(|_: Option<Object>, gpu_mem: u64| {
                        convert_storage(gpu_mem as f64, false)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        gpu_mem_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
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

        SETTINGS.connect_processes_show_gpu_memory(clone!(
            #[weak]
            gpu_mem_col,
            move |visible| gpu_mem_col.set_visible(visible)
        ));

        gpu_mem_col
    }

    fn add_total_cpu_time_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let total_cpu_time_col_factory = gtk::SignalListItemFactory::new();

        let total_cpu_time_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Total CPU Time")),
            Some(total_cpu_time_col_factory.clone()),
        );

        total_cpu_time_col.set_resizable(true);

        total_cpu_time_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);

                item.set_child(Some(&row));
                item.property_expression("item")
                    .chain_property::<ProcessEntry>("total_cpu_time")
                    .chain_closure::<String>(closure!(|_: Option<Object>, total_cpu_time: f64| {
                        format_time(total_cpu_time)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        total_cpu_time_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
        });

        let total_cpu_time_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "total_cpu_time",
            ))
            .build();

        total_cpu_time_col.set_sorter(Some(&total_cpu_time_col_sorter));
        total_cpu_time_col.set_visible(SETTINGS.processes_show_total_cpu_time());

        column_view.append_column(&total_cpu_time_col);

        SETTINGS.connect_processes_show_total_cpu_time(clone!(
            #[weak]
            total_cpu_time_col,
            move |visible| total_cpu_time_col.set_visible(visible)
        ));

        total_cpu_time_col
    }

    fn add_user_cpu_time_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let user_cpu_time_col_factory = gtk::SignalListItemFactory::new();

        let user_cpu_time_col = gtk::ColumnViewColumn::new(
            Some(&i18n("User CPU Time")),
            Some(user_cpu_time_col_factory.clone()),
        );

        user_cpu_time_col.set_resizable(true);

        user_cpu_time_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);

                item.set_child(Some(&row));
                item.property_expression("item")
                    .chain_property::<ProcessEntry>("user_cpu_time")
                    .chain_closure::<String>(closure!(|_: Option<Object>, user_cpu_time: f64| {
                        format_time(user_cpu_time)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        user_cpu_time_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
        });

        let user_cpu_time_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "user_cpu_time",
            ))
            .build();

        user_cpu_time_col.set_sorter(Some(&user_cpu_time_col_sorter));
        user_cpu_time_col.set_visible(SETTINGS.processes_show_user_cpu_time());

        column_view.append_column(&user_cpu_time_col);

        SETTINGS.connect_processes_show_user_cpu_time(clone!(
            #[weak]
            user_cpu_time_col,
            move |visible| user_cpu_time_col.set_visible(visible)
        ));

        user_cpu_time_col
    }

    fn add_system_cpu_time_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let system_cpu_time_col_factory = gtk::SignalListItemFactory::new();

        let system_cpu_time_col = gtk::ColumnViewColumn::new(
            Some(&i18n("System CPU Time")),
            Some(system_cpu_time_col_factory.clone()),
        );

        system_cpu_time_col.set_resizable(true);

        system_cpu_time_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);

                item.set_child(Some(&row));
                item.property_expression("item")
                    .chain_property::<ProcessEntry>("system_cpu_time")
                    .chain_closure::<String>(closure!(|_: Option<Object>, system_cpu_time: f64| {
                        format_time(system_cpu_time)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        system_cpu_time_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
        });

        let system_cpu_time_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "system_cpu_time",
            ))
            .build();

        system_cpu_time_col.set_sorter(Some(&system_cpu_time_col_sorter));
        system_cpu_time_col.set_visible(SETTINGS.processes_show_system_cpu_time());

        column_view.append_column(&system_cpu_time_col);

        SETTINGS.connect_processes_show_system_cpu_time(clone!(
            #[weak]
            system_cpu_time_col,
            move |visible| system_cpu_time_col.set_visible(visible)
        ));

        system_cpu_time_col
    }

    fn add_priority_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let priority_col_factory = gtk::SignalListItemFactory::new();

        let priority_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Priority")), Some(priority_col_factory.clone()));

        priority_col.set_resizable(true);

        priority_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(*LONGEST_PRIORITY_LABEL);

                item.set_child(Some(&row));
                item.property_expression("item")
                    .chain_property::<ProcessEntry>("niceness")
                    .chain_closure::<String>(closure!(|_: Option<Object>, niceness: i8| {
                        if SETTINGS.detailed_priority() {
                            niceness.to_string()
                        } else if let Ok(niceness) = Niceness::try_from(niceness) {
                            NICE_TO_LABEL
                                .get(&niceness)
                                .map(|(s, _)| s)
                                .cloned()
                                .unwrap_or_else(|| i18n("N/A"))
                        } else {
                            i18n("N/A")
                        }
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        priority_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
        });

        let priority_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "niceness",
            ))
            .build();

        priority_col.set_sorter(Some(&priority_col_sorter));
        priority_col.set_visible(SETTINGS.processes_show_priority());

        column_view.append_column(&priority_col);

        SETTINGS.connect_processes_show_priority(clone!(
            #[weak]
            priority_col,
            move |visible| priority_col.set_visible(visible)
        ));

        priority_col
    }

    fn add_swap_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let swap_col_factory = gtk::SignalListItemFactory::new();

        let swap_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Swap")), Some(swap_col_factory.clone()));

        swap_col.set_resizable(true);

        swap_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);
                row.set_xalign(1.0);

                item.set_child(Some(&row));

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("swap_usage")
                    .chain_closure::<String>(closure!(|_: Option<Object>, swap_usage: u64| {
                        convert_storage(swap_usage as f64, false)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        swap_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
        });

        let swap_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "swap_usage",
            ))
            .build();

        swap_col.set_sorter(Some(&swap_col_sorter));
        swap_col.set_visible(SETTINGS.processes_show_swap());

        column_view.append_column(&swap_col);

        SETTINGS.connect_processes_show_swap(clone!(
            #[weak]
            swap_col,
            move |visible| swap_col.set_visible(visible)
        ));

        swap_col
    }

    fn add_combined_memory_column(&self, column_view: &ColumnView) -> ColumnViewColumn {
        let combined_memory_col_factory = gtk::SignalListItemFactory::new();

        let combined_memory_col = gtk::ColumnViewColumn::new(
            Some(&i18n("Combined Memory")),
            Some(combined_memory_col_factory.clone()),
        );

        combined_memory_col.set_resizable(true);

        combined_memory_col_factory.connect_setup(clone!(
            #[weak(rename_to = this)]
            self,
            move |_factory, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();

                let row = gtk::Inscription::new(None);
                row.set_min_chars(9);
                row.set_xalign(1.0);

                item.set_child(Some(&row));

                item.property_expression("item")
                    .chain_property::<ProcessEntry>("combined_memory_usage")
                    .chain_closure::<String>(closure!(|_: Option<Object>, swap_usage: u64| {
                        convert_storage(swap_usage as f64, false)
                    }))
                    .bind(&row, "text", Widget::NONE);

                this.add_gestures(item);
            }
        ));

        combined_memory_col_factory.connect_teardown(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(None::<&gtk::Inscription>);
        });

        let combined_memory_col_sorter = NumericSorter::builder()
            .sort_order(SortType::Ascending)
            .expression(gtk::PropertyExpression::new(
                ProcessEntry::static_type(),
                None::<&gtk::Expression>,
                "combined_memory_usage",
            ))
            .build();

        combined_memory_col.set_sorter(Some(&combined_memory_col_sorter));
        combined_memory_col.set_visible(SETTINGS.processes_show_combined_memory());

        column_view.append_column(&combined_memory_col);

        SETTINGS.connect_processes_show_combined_memory(clone!(
            #[weak]
            combined_memory_col,
            move |visible| combined_memory_col.set_visible(visible)
        ));

        combined_memory_col
    }
}

fn get_action_name(action: ProcessAction, name: &str) -> String {
    match action {
        ProcessAction::TERM => i18n_f("End {}?", &[name]),
        ProcessAction::STOP => i18n_f("Halt {}?", &[name]),
        ProcessAction::KILL => i18n_f("Kill {}?", &[name]),
        ProcessAction::CONT => i18n_f("Continue {}?", &[name]),
        ProcessAction::KILLWINDOW => i18n("Kill a Window?"),
        ProcessAction::LOGOUT => i18n("Logout?"),
        ProcessAction::REBOOT => i18n("Reboot?"),
        ProcessAction::SHUTDOWN => i18n("Shutdown?"),
    }
}

fn get_action_name_multiple(action: ProcessAction, count: usize) -> String {
    match action {
        ProcessAction::TERM => ni18n_f(
            "End process?",
            "End {} processes?",
            count as u32,
            &[&count.to_string()],
        ),
        ProcessAction::STOP => ni18n_f(
            "Halt process?",
            "Halt {} processes?",
            count as u32,
            &[&count.to_string()],
        ),
        ProcessAction::KILL => ni18n_f(
            "Kill process?",
            "Kill {} processes?",
            count as u32,
            &[&count.to_string()],
        ),
        ProcessAction::CONT => ni18n_f(
            "Kill process?",
            "Kill {} processes?",
            count as u32,
            &[&count.to_string()],
        ),
        ProcessAction::KILLWINDOW => i18n("Kill a Window?"),
        ProcessAction::LOGOUT => i18n("Logout?"),
        ProcessAction::REBOOT => i18n("Reboot?"),
        ProcessAction::SHUTDOWN => i18n("Shutdown?"),
    }
}

fn get_action_warning(action: ProcessAction) -> String {
    match action {
        ProcessAction::TERM => i18n("Unsaved work might be lost."),
        ProcessAction::STOP => i18n(
            "Halting a process can come with serious risks such as losing data and security implications. Use with caution.",
        ),
        ProcessAction::KILL => i18n(
            "Killing a process can come with serious risks such as losing data and security implications. Use with caution.",
        ),
        ProcessAction::CONT => String::new(),
        ProcessAction::KILLWINDOW => i18n("Click on a window to kill it."),
        ProcessAction::LOGOUT => {
            i18n("This action will be executed without checking for unsaved files.")
        }
        ProcessAction::REBOOT => {
            i18n("This action will be executed without checking for unsaved files.")
        }
        ProcessAction::SHUTDOWN => {
            i18n("This action will be executed without checking for unsaved files.")
        }
    }
}

fn get_action_description(action: ProcessAction) -> String {
    match action {
        ProcessAction::TERM => i18n("End Process"),
        ProcessAction::STOP => i18n("Halt Process"),
        ProcessAction::KILL => i18n("Kill Process"),
        ProcessAction::CONT => i18n("Continue Process"),
        ProcessAction::KILLWINDOW => i18n("Kill Window"),
        ProcessAction::LOGOUT => i18n("Logout"),
        ProcessAction::REBOOT => i18n("Reboot"),
        ProcessAction::SHUTDOWN => i18n("Shutdown"),
    }
}
