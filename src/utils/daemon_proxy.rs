use std::collections::BTreeMap;

use anyhow::{Context, Result};
use zbus::{Connection, dbus_proxy};

#[dbus_proxy(
    default_service = "me.nalux.Resources",
    interface = "me.nalux.Resources",
    default_path = "/me/nalux/Resources"
)]
trait Client {
    fn ram_info(&self) -> Result<Vec<BTreeMap<String, String>>>;
}

pub async fn dbus_ram_info() -> Result<Vec<BTreeMap<String, String>>> {
    let conn = Connection::system()
        .await
        .context("error trying to establish dbus system connection")?;
    let proxy = ClientProxy::new(&conn)
        .await
        .context("error trying to build new dbus ClientProxy")?;
    proxy
        .ram_info()
        .await
        .context("error calling dbus proxy with `ram_info`, is the daemon running?")
}