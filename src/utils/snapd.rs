use std::{cell::RefCell, collections::HashMap, path::Path};

use futures::future::{FutureExt, LocalBoxFuture, Shared};
use gtk::{gio, glib};
use log::debug;
use soup::prelude::*;

const SNAPD_SOCKET: &str = "/var/run/snapd.socket";

/// A cloneable handle to an in-flight or completed snapd desktop-ID query.
pub(super) type SnapFuture = Shared<LocalBoxFuture<'static, Option<String>>>;

thread_local! {
    static SNAPD_SESSION: soup::Session = soup::Session::builder()
        .remote_connectable(&gio::UnixSocketAddress::new(Path::new(SNAPD_SOCKET)))
        .timeout(3)
        .build();
    static DESKTOP_ID_CACHE: RefCell<HashMap<(String, String), SnapFuture>> =
        RefCell::new(HashMap::new());
}

/// Returns a `SnapFuture` for the given snap app's freedesktop desktop ID.
/// On the first call, creates the future, spawns it on the GLib main context, and caches it.
/// Subsequent calls return the same (possibly already-resolved) shared future.
pub(super) fn get_desktop_id(snap_name: &str, snap_app_name: &str) -> SnapFuture {
    DESKTOP_ID_CACHE.with_borrow_mut(|cache| {
        cache
            .entry((snap_name.to_string(), snap_app_name.to_string()))
            .or_insert_with(|| {
                let snap_name = snap_name.to_string();
                let snap_app_name = snap_app_name.to_string();
                let fut = async move { get_desktop_id_uncached(&snap_name, &snap_app_name).await }
                    .boxed_local()
                    .shared();
                glib::MainContext::default().spawn_local(fut.clone().map(|_| ()));
                fut
            })
            .clone()
    })
}

async fn get_desktop_id_uncached(snap_name: &str, snap_app_name: &str) -> Option<String> {
    debug!("Querying snapd for {snap_name}.{snap_app_name} → desktop ID");

    let message = soup::Message::new("GET", &format!("http://localhost/v2/snaps/{snap_name}"))
        .inspect_err(|e| debug!("Failed to create snapd message: {e}"))
        .ok()?;
    if let Some(headers) = message.request_headers() {
        headers.append("X-Allow-Interaction", "false");
    }

    let bytes = SNAPD_SESSION
        .with(|session| session.send_and_read_future(&message, glib::Priority::DEFAULT))
        .await
        .inspect_err(|e| debug!("snapd request for {snap_name} failed: {e}"))
        .ok()?;

    let json: serde_json::Value = serde_json::from_slice(bytes.as_ref())
        .inspect_err(|e| debug!("Failed to parse snapd JSON for {snap_name}: {e}"))
        .ok()?;

    let desktop_file = json["result"]["apps"]
        .as_array()?
        .iter()
        .find(|app| app["name"].as_str() == Some(snap_app_name))?["desktop-file"]
        .as_str()?;

    let id = Path::new(desktop_file).file_stem()?.to_str()?.to_owned();
    debug!("Resolved snap {snap_name}.{snap_app_name} → desktop ID: {id}");
    Some(id)
}
