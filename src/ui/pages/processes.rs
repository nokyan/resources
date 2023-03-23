use std::cell::Ref;

use adw::{prelude::*, subclass::prelude::*};
use adw::{ResponseAppearance, Toast};
use gtk::glib::{self, clone, timeout_future_seconds, BoxedAnyObject, MainContext, Object};
use gtk::{gio, CustomSorter, FilterChange, Ordering, SortType};

use crate::config::PROFILE;
use crate::i18n::{i18n, i18n_f};
use crate::ui::dialogs::process_dialog::ResProcessDialog;
use crate::ui::widgets::process_name_cell::ResProcessNameCell;
use crate::ui::window::MainWindow;
use crate::utils::processes::{self, Apps, Process};
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use std::{cell::RefCell, collections::HashMap};

    use super::*;

    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
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

        pub apps: RefCell<Apps>,
        pub store: RefCell<gio::ListStore>,
        pub selection_model: RefCell<gtk::SingleSelection>,
        pub filter_model: RefCell<gtk::FilterListModel>,
        pub sort_model: RefCell<gtk::SortListModel>,
        pub column_view: RefCell<gtk::ColumnView>,
        pub open_dialog: RefCell<Option<(i32, ResProcessDialog)>>,
        pub uid_map: RefCell<HashMap<u32, String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResProcesses {
        const NAME: &'static str = "ResProcesses";
        type Type = super::ResProcesses;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action("processes.kill-process", None, move |resprocesses, _, _| {
                resprocesses.kill_selected_process();
            });

            klass.install_action("processes.halt-process", None, move |resprocesses, _, _| {
                resprocesses.halt_selected_process();
            });

            klass.install_action(
                "processes.continue-process",
                None,
                move |resprocesses, _, _| {
                    resprocesses.continue_selected_process();
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

    pub fn init(&self) {
        self.setup_widgets();
        self.setup_signals();
        self.setup_listener();
    }

    fn get_user_name_by_uid(&self, uid: u32) -> String {
        let imp = self.imp();
        // cache all the user names so we don't have
        // to do expensive lookups all the time
        (*imp.uid_map.borrow_mut().entry(uid).or_insert_with(|| {
            users::get_user_by_uid(uid).map_or_else(
                || i18n("root"),
                |user| user.name().to_string_lossy().to_string(),
            )
        }))
        .to_string() // TODO: remove .to_string() with something more efficient
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();

        let column_view = gtk::ColumnView::new(None::<gtk::SingleSelection>);
        let store = gio::ListStore::new(BoxedAnyObject::static_type());
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
        *imp.store.borrow_mut() = store;

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
            let r: Ref<Process> = entry.borrow();
            child.set_name(processes::Process::sanitize_cmdline(&r.comm));
            child.set_icon(Some(&r.icon));
            child.set_tooltip(Some(&r.commandline));
        });
        let name_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>();
            item_a.comm.cmp(&item_b.comm).into()
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
            let r: Ref<Process> = entry.borrow();
            child.set_text(Some(&r.pid.to_string()));
        });
        let pid_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>();
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
            let r: Ref<Process> = entry.borrow();
            child.set_text(Some(&this.get_user_name_by_uid(r.uid)));
        }));
        let user_col_sorter = CustomSorter::new(clone!(@strong self as this => move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>();
            let user_a = this.get_user_name_by_uid(item_a.uid);
            let user_b = this.get_user_name_by_uid(item_b.uid);
            user_a.cmp(&user_b).into()
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
            let r: Ref<Process> = entry.borrow();
            let (number, prefix) = to_largest_unit(r.memory_usage as f64, &Base::Decimal);
            child.set_text(Some(&format!("{number:.1} {prefix}B")));
        });
        let memory_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>();
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
            let r: Ref<Process> = entry.borrow();
            child.set_text(Some(&format!("{:.1} %", r.cpu_time_ratio() * 100.0)));
        });
        let cpu_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>()
                .cpu_time_ratio();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<Process>()
                .cpu_time_ratio();
            // we have to do this because f32s do not implement `Ord`
            if item_a > item_b {
                Ordering::Larger
            } else if item_a < item_b {
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

        *imp.apps.borrow_mut() = futures::executor::block_on(Apps::new()).unwrap();
        imp.apps
            .borrow()
            .all_processes()
            .iter()
            .map(|process| BoxedAnyObject::new(process.clone()))
            .for_each(|item_box| imp.store.borrow().append(&item_box));
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
                    filter.changed(FilterChange::Different)
                }
                if button.is_active() {
                    imp.search_entry.grab_focus();
                }
            }));

        imp.search_entry
            .connect_search_changed(clone!(@strong self as this => move |_| {
                let imp = this.imp();
                if let Some(filter) = imp.filter_model.borrow().filter() {
                    filter.changed(FilterChange::Different)
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
                    .borrow::<Process>()
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
                this.end_selected_process();
            }));
    }

    pub fn setup_listener(&self) {
        // TODO: don't use unwrap()
        let main_context = MainContext::default();
        main_context.spawn_local(clone!(@strong self as this => async move {
            loop {
                timeout_future_seconds(2).await;
                this.refresh_processes_list().await;
            }
        }));
    }

    fn search_filter(&self, obj: &Object) -> bool {
        let imp = self.imp();
        let item = obj
            .downcast_ref::<BoxedAnyObject>()
            .unwrap()
            .borrow::<Process>()
            .clone();
        let search_string = imp.search_entry.text().to_string().to_lowercase();
        !imp.search_revealer.reveals_child()
            || item.comm.to_lowercase().contains(&search_string)
            || item.commandline.to_lowercase().contains(&search_string)
    }

    fn get_selected_process(&self) -> Option<Process> {
        self.imp()
            .selection_model
            .borrow()
            .selected_item()
            .map(|object| {
                object
                    .downcast::<BoxedAnyObject>()
                    .unwrap()
                    .borrow::<Process>()
                    .clone()
            })
    }

    async fn refresh_processes_list(&self) {
        let imp = self.imp();
        let selection = imp.selection_model.borrow();
        let mut apps = imp.apps.borrow_mut();

        // if we reuse the old ListStore, for some reason setting the
        // vadjustment later just doesn't work most of the time.
        // so we just make a new one every refresh instead :')
        // TODO: make this less hacky
        let new_store = gio::ListStore::new(BoxedAnyObject::static_type());

        // this might be very hacky, but remember the ID of the currently
        // selected item, clear the list model and repopulate it with the
        // refreshed apps and stats, then reselect the remembered app.
        // TODO: make this even less hacky
        let selected_item = self.get_selected_process().map(|process| process.pid);
        if apps.refresh().await.is_ok() {
            apps.all_processes()
                .iter()
                .filter(|process| !process.commandline.is_empty())
                .map(|process| {
                    if let Some((pid, dialog)) = &*imp.open_dialog.borrow() && process.pid == *pid {
                        dialog.set_cpu_usage(process.cpu_time_ratio());
                        dialog.set_memory_usage(process.memory_usage);
                    }
                    BoxedAnyObject::new(process.clone())
                })
                .for_each(|item_box| new_store.append(&item_box));
        }
        imp.filter_model.borrow().set_model(Some(&new_store));
        *imp.store.borrow_mut() = new_store;

        // find the (potentially) new index of the process that was selected
        // before the refresh and set our selection to that index
        if let Some(selected_item) = selected_item {
            let new_index = selection
                .iter::<glib::Object>()
                .position(|object| {
                    object
                        .unwrap()
                        .downcast::<BoxedAnyObject>()
                        .unwrap()
                        .borrow::<Process>()
                        .pid
                        == selected_item
                })
                .map(|index| index as u32);
            if let Some(index) = new_index && index != u32::MAX {
                selection.set_selected(index);
            }
        }
    }

    fn end_selected_process(&self) {
        let selection_option = self.get_selected_process();
        if let Some(process) = selection_option {
            let dialog = adw::MessageDialog::builder()
                .transient_for(&MainWindow::default())
                .modal(true)
                .heading(i18n_f("End {}?", &[&process.comm]))
                .body(i18n("Unsaved work might be lost."))
                .build();
            dialog.add_response("yes", &i18n("End Process"));
            dialog.set_response_appearance("yes", ResponseAppearance::Destructive);
            dialog.set_default_response(Some("no"));
            dialog.add_response("no", &i18n("Cancel"));
            dialog.set_close_response("no");
            dialog.connect_response(None, clone!(@strong process, @weak self as this => move |_, response| {
                if response == "yes" {
                    let imp = this.imp();
                    match process.term() {
                        Ok(_) => { imp.toast_overlay.add_toast(Toast::new(&i18n_f("Successfully ended {}", &[&process.comm]))); },
                        Err(_) => { imp.toast_overlay.add_toast(Toast::new(&i18n_f("There was a problem ending {}", &[&process.comm]))); },
                    };
                }
            }));
            dialog.show();
        }
    }

    fn kill_selected_process(&self) {
        let selection_option = self.get_selected_process();
        if let Some(process) = selection_option {
            let dialog = adw::MessageDialog::builder()
            .transient_for(&MainWindow::default())
            .modal(true)
            .heading(i18n_f("Kill {}?", &[&process.comm]))
            .body(i18n("Killing a process can come with serious risks such as losing data and security implications. Use with caution."))
            .build();
            dialog.add_response("yes", &i18n("Kill Process"));
            dialog.set_response_appearance("yes", ResponseAppearance::Destructive);
            dialog.set_default_response(Some("no"));
            dialog.add_response("no", &i18n("Cancel"));
            dialog.set_close_response("no");
            dialog.connect_response(None, clone!(@strong process, @weak self as this => move |_, response| {
                if response == "yes" {
                    let imp = this.imp();
                    match process.kill() {
                        Ok(_) => { imp.toast_overlay.add_toast(Toast::new(&i18n_f("Successfully killed {}", &[&process.comm]))); },
                        Err(_) => { imp.toast_overlay.add_toast(Toast::new(&i18n_f("There was a problem killing {}", &[&process.comm]))); },
                    };
                }
            }));
            dialog.show();
        }
    }

    fn halt_selected_process(&self) {
        let selection_option = self.get_selected_process();
        if let Some(process) = selection_option {
            let dialog = adw::MessageDialog::builder()
            .transient_for(&MainWindow::default())
            .modal(true)
            .heading(i18n_f("Halt {}?", &[&process.comm]))
            .body(i18n("Halting a process can come with serious risks such as losing data and security implications. Use with caution."))
            .build();
            dialog.add_response("yes", &i18n("Halt Process"));
            dialog.set_response_appearance("yes", ResponseAppearance::Destructive);
            dialog.set_default_response(Some("no"));
            dialog.add_response("no", &i18n("Cancel"));
            dialog.set_close_response("no");
            dialog.connect_response(None, clone!(@strong process, @weak self as this => move |_, response| {
                if response == "yes" {
                    let imp = this.imp();
                    match process.stop() {
                        Ok(_) => { imp.toast_overlay.add_toast(Toast::new(&i18n_f("Successfully halted {}", &[&process.comm]))); },
                        Err(_) => { imp.toast_overlay.add_toast(Toast::new(&i18n_f("There was a problem halting {}", &[&process.comm]))); },
                    };
                }
            }));
            dialog.show();
        }
    }

    fn continue_selected_process(&self) {
        let imp = self.imp();
        let selection_option = self.get_selected_process();
        if let Some(process) = selection_option {
            match process.cont() {
                Ok(_) => {
                    imp.toast_overlay.add_toast(Toast::new(&i18n_f(
                        "Successfully continued {}",
                        &[&process.comm],
                    )));
                }
                Err(_) => {
                    imp.toast_overlay.add_toast(Toast::new(&i18n_f(
                        "There was a problem continuing {}",
                        &[&process.comm],
                    )));
                }
            };
        }
    }
}
