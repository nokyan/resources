use anyhow::{Context, Result};
use glob::glob;
use process_data::ProcessData;

#[async_std::main]
async fn main() -> Result<()> {
    let mut return_vec = Vec::new();
    for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
        if let Ok(process_data) = ProcessData::try_from_path(entry).await {
            return_vec.push(process_data);
        }
    }
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    rmp_serde::encode::write(&mut handle, &return_vec).unwrap();
    Ok(())
}
