use anyhow::Result;
use process_data::ProcessData;
use rlimit::Resource;
use std::io::Write;
use std::time::Duration;
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> Result<()> {
    // when there are a couple hundred processes running, we might run into the file descriptor limit
    if let Ok(hard) = Resource::NOFILE.get_hard() {
        let _ = Resource::NOFILE.set(hard, hard);
    }

    loop {
        let mut buffer = [0; 1];

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::io::stdin().read_exact(&mut buffer),
        )
        .await;

        match result {
            Ok(_) => {
                let data = ProcessData::all_process_data().await?;
                let encoded = rmp_serde::encode::to_vec(&*data)?;

                let len_byte_array = encoded.len().to_le_bytes();

                let stdout = std::io::stdout();
                let mut handle = stdout.lock();

                let _ = handle.write_all(&len_byte_array);

                let _ = handle.write_all(&encoded);

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
        let data = ProcessData::all_process_data().await?;
        let encoded = rmp_serde::encode::to_vec(&*data)?;
        std::hint::black_box(encoded);
    }*/

    Ok(())
}
