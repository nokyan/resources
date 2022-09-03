use zbus::{ConnectionBuilder, Result};

use self::server::SystemResourcesDaemon;

pub mod server;

pub async fn main() -> Result<()> {
    let _ = ConnectionBuilder::system()?
        .name("me.nalux.Resources")?
        .serve_at("/me/nalux/Resources", SystemResourcesDaemon)?
        .build()
        .await?;
    Ok(())
}
