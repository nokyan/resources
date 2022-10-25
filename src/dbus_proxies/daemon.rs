use std::collections::BTreeMap;

use anyhow::Result;
use zbus::dbus_proxy;

#[dbus_proxy(
    default_service = "me.nalux.Resources",
    interface = "me.nalux.Resources",
    default_path = "/me/nalux/Resources"
)]
trait Client {
    fn ram_info(&self) -> Result<Vec<BTreeMap<String, String>>>;
    fn probe_drives(&self) -> Result<Vec<String>>;
}
