use anyhow::Result;
use process_data::ProcessData;
use std::io::{Read, Write};

fn main() -> Result<()> {
    loop {
        let mut buffer = [0; 1];

        std::io::stdin().read_exact(&mut buffer)?;

        let data = ProcessData::all_process_data()?;
        let encoded = rmp_serde::encode::to_vec(&*data)?;

        let len_byte_array = encoded.len().to_le_bytes();

        let stdout = std::io::stdout();
        let mut handle = stdout.lock();

        handle.write_all(&len_byte_array)?;

        handle.write_all(&encoded)?;

        handle.flush()?;
    }
}
