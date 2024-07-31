extern crate paste;

#[macro_export]
macro_rules! gstring_getter_setter {
    ($($gstring_name:ident),*) => {
        $(
            pub fn $gstring_name(&self) -> glib::GString {
                let $gstring_name = self.$gstring_name.take();
                self.$gstring_name.set($gstring_name.clone());
                $gstring_name
            }

            paste::paste! {
                pub fn [<set_ $gstring_name>](&self, $gstring_name: &str) {
                    self.$gstring_name.set(glib::GString::from($gstring_name));
                }
            }
        )*
    };
}

#[macro_export]
macro_rules! gstring_option_getter_setter {
    ($($gstring_name:ident),*) => {
        $(
            pub fn $gstring_name(&self) -> Option<glib::GString> {
                let $gstring_name = self.$gstring_name.take();
                self.$gstring_name.set($gstring_name.clone());
                $gstring_name
            }

            paste::paste! {
                pub fn [<set_ $gstring_name>](&self, $gstring_name: Option<&str>) {
                    self.$gstring_name.set($gstring_name.map(glib::GString::from));
                }
            }
        )*
    };
}

pub mod dialogs;
pub mod pages;
pub mod widgets;
pub mod window;
