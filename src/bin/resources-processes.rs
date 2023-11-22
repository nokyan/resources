use anyhow::{Context, Result};
use glob::glob;
use process_data::ProcessData;

fn main() -> Result<()> {
    let mut process_data = vec![];
    for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
        if let Ok(data) = ProcessData::try_from_path(entry) {
            process_data.push(data);
        }
    }

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    rmp_serde::encode::write(&mut handle, &*process_data).unwrap();

    Ok(())
}
