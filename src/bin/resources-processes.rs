use anyhow::{Context, Result};
use futures::future::join_all;
use glob::glob;
use process_data::ProcessData;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    loop {
        let mut buffer = [0; 1];

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::io::stdin().read_exact(&mut buffer),
        )
        .await;

        match result {
            Ok(_) => {
                let data = process_data_as_bytes().await.unwrap_or_default();

                let len_byte_array = data.len().to_le_bytes();

                let stdout = std::io::stdout();
                let mut handle = stdout.lock();

                let _ = handle.write_all(&len_byte_array);

                let _ = handle.write_all(&data);

                if let Err(_) = handle.flush() {
                    break;
                }
            }
            _ => {
                // No input in 10 seconds, exit the loop
                break;
            }
        }
    }

    /*for _ in 0..15 {
        std::hint::black_box(process_data_as_bytes().await.unwrap_or_default());
    }*/

    Ok(())
}

async fn process_data_as_bytes() -> Result<Vec<u8>> {
    let return_vec = Arc::new(Mutex::new(Vec::new()));

    ProcessData::update_nvidia_stats().await;

    let mut handles = vec![];
    for entry in glob("/proc/[0-9]*/").context("unable to glob")?.flatten() {
        let return_vec = Arc::clone(&return_vec);

        let handle = tokio::task::spawn(async move {
            if let Ok(process_data) = ProcessData::try_from_path(entry).await {
                return_vec.lock().await.push(process_data);
            }
        });

        handles.push(handle);
    }
    join_all(handles).await;

    let return_vec = return_vec.lock().await;

    let encoded = rmp_serde::encode::to_vec(&*return_vec)?;

    Ok(encoded)
}
