use anyhow::{Context, Result};
use glob::glob;
use process_data::ProcessData;
use tokio::task::JoinSet;

#[tokio::main]
async fn main() -> Result<()> {
    let mut tasks = JoinSet::new();

    for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
        tasks.spawn(async move { ProcessData::try_from_path(entry).await });
    }

    let mut process_data = vec![];
    while let Some(task) = tasks.join_next().await {
        if let Ok(data) = task.unwrap() {
            process_data.push(data);
        }
    }

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    rmp_serde::encode::write(&mut handle, &*process_data).unwrap();

    Ok(())
}
