use std::error::Error;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    resources::daemon::main().await?;
    std::future::pending::<()>().await;
    Ok(())
}
