use anyhow::Result;
use process_data::ProcessData;
use ron::ser::PrettyConfig;
use std::io::{Read, Write};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Output once and then exit
    #[arg(short, long, default_value_t = false)]
    once: bool,

    /// Use Rusty Object Notation (use this only for debugging this binary on its own, Resources won't be able to decode RON)
    #[arg(short, long, default_value_t = false)]
    ron: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.once {
        output(args.ron)?;
        return Ok(());
    }

    loop {
        let mut buffer = [0; 1];

        std::io::stdin().read_exact(&mut buffer)?;

        output(args.ron)?;
    }
}

fn output(ron: bool) -> Result<()> {
    let data = ProcessData::all_process_data()?;

    let encoded = if ron {
        ron::ser::to_string_pretty(&data, PrettyConfig::default())?
            .as_bytes()
            .to_vec()
    } else {
        rmp_serde::to_vec(&data)?
    };

    let len_byte_array = encoded.len().to_le_bytes();

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    handle.write_all(&len_byte_array)?;

    handle.write_all(&encoded)?;

    handle.flush()?;
    Ok(())
}
