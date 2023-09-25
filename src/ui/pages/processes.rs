use std::cell::Ref;
use std::collections::HashSet;

use adw::ResponseAppearance;
use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, BoxedAnyObject, Object, Sender};
use gtk::{gio, CustomSorter, FilterChange, Ordering, SelectionModel, SortType};
use gtk_macros::send;

use log::error;

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::dialogs::process_dialog::ResProcessDialog;
use crate::ui::widgets::process_name_cell::ResProcessNameCell;
use crate::ui::window::{self, Action, MainWindow};
use crate::utils::processes::{AppsContext, ProcessAction, ProcessItem};
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use std::{cell::RefCell, collections::HashMap, sync::OnceLock};

    use crate::{ui::window::Action, utils::processes::ProcessAction};

    use super::*;

    use gtk::{glib::Sender, CompositeTemplate};

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/me/nalux/Resources/ui/pages/processes.ui")]
    pub struct ResProcesses {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
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
                store: gio::ListStore::new::<BoxedAnyObject>().into(),
                selection_model: Default::default(),
                filter_model: Default::default(),
                sort_model: Default::default(),
                column_view: Default::default(),
                open_dialog: Default::default(),
                username_cache: Default::default(),
                sender: Default::default(),
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
                "processes.kill-process",
                None,
                move |res_processes, _, _| {
                    if let Some(app) = res_processes.get_selected_process_item() {
                        res_processes.execute_process_action_dialog(app, ProcessAction::KILL);
                    }
                },
            );

            klass.install_action(
                "processes.halt-process",
                None,
                move |res_processes, _, _| {
                    if let Some(app) = res_processes.get_selected_process_item() {
                        res_processes.execute_process_action_dialog(app, ProcessAction::STOP);
                    }
                },
            );

            klass.install_action(
                "processes.continue-process",
                None,
                move |res_processes, _, _| {
                    if let Some(app) = res_processes.get_selected_process_item() {
                        res_processes.execute_process_action_dialog(app, ProcessAction::CONT);
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

    pub fn init(&self, sender: Sender<Action>) {
        let imp = self.imp();
        imp.sender.set(sender).unwrap();

        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();

        let column_view = gtk::ColumnView::new(None::<gtk::SingleSelection>);
        let store = imp.store.borrow_mut();
        let filter_model = gtk::FilterListModel::new(
            Some(store.clone()),
            Some(gtk::CustomFilter::new(
                clone!(@strong self as this => move |obj| this.search_filter(obj)),
            )),
        );
        let sort_model = gtk::SortListModel::new(Some(filter_model.clone()), column_view.sorter());
        let selection_model = gtk::SingleSelection::new(Some(sort_model.clone()));
        column_view.set_model(Some(&selection_model));
        selection_model.set_can_unselect(true);
        selection_model.set_autoselect(false);

        *imp.selection_model.borrow_mut() = selection_model;
        *imp.sort_model.borrow_mut() = sort_model;
        *imp.filter_model.borrow_mut() = filter_model;

        let name_col_factory = gtk::SignalListItemFactory::new();
        let name_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Process")), Some(name_col_factory.clone()));
        name_col.set_resizable(true);
        name_col.set_expand(true);
        name_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = ResProcessNameCell::new();
            item.set_child(Some(&row));
        });
        name_col_factory.connect_bind(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let child = item
                .child()
                .unwrap()
                .downcast::<ResProcessNameCell>()
                .unwrap();
            let entry = item.item().unwrap().downcast::<BoxedAnyObject>().unwrap();
            let r: Ref<ProcessItem> = entry.borrow();
            child.set_name(&r.display_name);
            child.set_icon(Some(&r.icon));
            child.set_tooltip(Some(&r.commandline));
        });
        let name_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>();
            item_a.display_name.cmp(&item_b.display_name).into()
        });
        name_col.set_sorter(Some(&name_col_sorter));

        let pid_col_factory = gtk::SignalListItemFactory::new();
        let pid_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Process ID")), Some(pid_col_factory.clone()));
        pid_col.set_resizable(true);
        pid_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = gtk::Inscription::new(None);
            item.set_child(Some(&row));
        });
        pid_col_factory.connect_bind(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let child = item
                .child()
                .unwrap()
                .downcast::<gtk::Inscription>()
                .unwrap();
            let entry = item.item().unwrap().downcast::<BoxedAnyObject>().unwrap();
            let r: Ref<ProcessItem> = entry.borrow();
            child.set_text(Some(&r.pid.to_string()));
        });
        let pid_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>();
            item_a.pid.cmp(&item_b.pid).into()
        });
        pid_col.set_sorter(Some(&pid_col_sorter));

        let user_col_factory = gtk::SignalListItemFactory::new();
        let user_col =
            gtk::ColumnViewColumn::new(Some(&i18n("User")), Some(user_col_factory.clone()));
        user_col.set_resizable(true);
        user_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = gtk::Inscription::new(None);
            item.set_child(Some(&row));
        });
        user_col_factory.connect_bind(clone!(@strong self as this => move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let child = item
                .child()
                .unwrap()
                .downcast::<gtk::Inscription>()
                .unwrap();
            let entry = item.item().unwrap().downcast::<BoxedAnyObject>().unwrap();
            let r: Ref<ProcessItem> = entry.borrow();
            child.set_text(Some(&this.get_user_name_by_uid(r.uid)));
        }));
        let user_col_sorter = CustomSorter::new(clone!(@strong self as this => move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>();
            let user_a = &this.get_user_name_by_uid(item_a.uid);
            let user_b = &this.get_user_name_by_uid(item_b.uid);
            user_a.cmp(user_b).into()
        }));
        user_col.set_sorter(Some(&user_col_sorter));

        let memory_col_factory = gtk::SignalListItemFactory::new();
        let memory_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Memory")), Some(memory_col_factory.clone()));
        memory_col.set_resizable(true);
        memory_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = gtk::Inscription::new(None);
            item.set_child(Some(&row));
        });
        memory_col_factory.connect_bind(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let child = item
                .child()
                .unwrap()
                .downcast::<gtk::Inscription>()
                .unwrap();
            let entry = item.item().unwrap().downcast::<BoxedAnyObject>().unwrap();
            let r: Ref<ProcessItem> = entry.borrow();
            let (number, prefix) = to_largest_unit(r.memory_usage as f64, &Base::Decimal);
            child.set_text(Some(&format!("{number:.1} {prefix}B")));
        });
        let memory_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>();
            item_a.memory_usage.cmp(&item_b.memory_usage).into()
        });
        memory_col.set_sorter(Some(&memory_col_sorter));

        let cpu_col_factory = gtk::SignalListItemFactory::new();
        let cpu_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Processor")), Some(cpu_col_factory.clone()));
        cpu_col.set_resizable(true);
        cpu_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = gtk::Inscription::new(None);
            item.set_child(Some(&row));
        });
        cpu_col_factory.connect_bind(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let child = item
                .child()
                .unwrap()
                .downcast::<gtk::Inscription>()
                .unwrap();
            let entry = item.item().unwrap().downcast::<BoxedAnyObject>().unwrap();
            let r: Ref<ProcessItem> = entry.borrow();
            child.set_text(Some(&format!("{:.1} %", r.cpu_time_ratio * 100.0)));
        });
        let cpu_col_sorter = CustomSorter::new(move |a, b| {
            let ratio_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>()
                .cpu_time_ratio;
            let ratio_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<ProcessItem>()
                .cpu_time_ratio;
            // we have to do this because f32s do not implement `Ord`
            if ratio_a > ratio_b {
                Ordering::Larger
            } else if ratio_a < ratio_b {
                Ordering::Smaller
            } else {
                Ordering::Equal
            }
        });
        cpu_col.set_sorter(Some(&cpu_col_sorter));

        column_view.append_column(&name_col);
        column_view.append_column(&pid_col);
        column_view.append_column(&user_col);
        column_view.append_column(&memory_col);
        column_view.append_column(&cpu_col);
        column_view.sort_by_column(Some(&cpu_col), SortType::Descending);
        column_view.set_enable_rubberband(true);
        imp.processes_scrolled_window.set_child(Some(&column_view));
        *imp.column_view.borrow_mut() = column_view;
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
                    .downcast::<BoxedAnyObject>()
                    .unwrap()
                    .borrow::<ProcessItem>()
                    .clone()
                });
                if let Some(selection) = selection_option {
                    let process_dialog = ResProcessDialog::new();
                    process_dialog.init(&selection, this.get_user_name_by_uid(selection.uid));
                    process_dialog.show();
                    *imp.open_dialog.borrow_mut() = Some((selection.pid, process_dialog));
                }
            }));

        imp.end_process_button
            .connect_clicked(clone!(@strong self as this => move |_| {
                if let Some(app) = this.get_selected_process_item() {
                    this.execute_process_action_dialog(app, ProcessAction::TERM);
                }
            }));
    }

    fn search_filter(&self, obj: &Object) -> bool {
        let imp = self.imp();
        let item = obj
            .downcast_ref::<BoxedAnyObject>()
            .unwrap()
            .borrow::<ProcessItem>()
            .clone();
        let search_string = imp.search_entry.text().to_string().to_lowercase();
        !imp.search_revealer.reveals_child()
            || item.display_name.to_lowercase().contains(&search_string)
            || item.commandline.to_lowercase().contains(&search_string)
    }

    fn get_selected_process_item(&self) -> Option<ProcessItem> {
        self.imp()
            .selection_model
            .borrow()
            .selected_item()
            .map(|object| {
                object
                    .downcast::<BoxedAnyObject>()
                    .unwrap()
                    .borrow::<ProcessItem>()
                    .clone()
            })
    }

    pub fn refresh_processes_list(&self, apps: &AppsContext) {
        let imp = self.imp();

        let store = imp.store.borrow_mut();
        let mut dialog_opt = &*imp.open_dialog.borrow_mut();

        let mut new_items = apps.process_items();
        let mut pids_to_remove = HashSet::new();

        // change process entries of processes that have existed before
        store.iter::<BoxedAnyObject>().flatten().for_each(|object| {
            let item_pid = object.borrow::<ProcessItem>().pid;
            // filter out processes that have existed before but don't anymore
            if !apps.get_process(item_pid).unwrap().alive {
                if let Some((dialog_pid, dialog)) = dialog_opt && *dialog_pid == item_pid {
                    dialog.close();
                    dialog_opt = &None;
                }
                pids_to_remove.insert(item_pid);
            }
            if let Some((_, new_item)) = new_items.remove_entry(&item_pid) {
                if let Some((dialog_pid, dialog)) = dialog_opt && *dialog_pid == item_pid {
                    dialog.set_cpu_usage(new_item.cpu_time_ratio);
                    dialog.set_memory_usage(new_item.memory_usage);
                }
                object.replace(new_item);
            }
        });

        // remove recently deceased processes
        store.retain(|object| {
            !pids_to_remove.contains(
                &object
                    .clone()
                    .downcast::<BoxedAnyObject>()
                    .unwrap()
                    .borrow::<ProcessItem>()
                    .pid,
            )
        });

        // add the newly started process to the store
        new_items
            .drain()
            .for_each(|(_, new_item)| store.append(&BoxedAnyObject::new(new_item)));

        store.items_changed(0, store.n_items(), store.n_items());
        imp.column_view
            .borrow()
            .set_model(None::<SelectionModel>.as_ref());
        imp.column_view
            .borrow()
            .set_model(Some(&*imp.selection_model.borrow()));
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

        dialog.show();
    }

    fn get_user_name_by_uid(&self, uid: u32) -> String {
        let imp = self.imp();
        // cache all the user names so we don't have
        // to do expensive lookups all the time
        (imp.username_cache
            .borrow_mut()
            .entry(uid)
            .or_insert_with(|| {
                uzers::get_user_by_uid(uid).map_or_else(
                    || i18n("root"),
                    |user| user.name().to_string_lossy().to_string(),
                )
            }))
        .to_string()
    }
}
