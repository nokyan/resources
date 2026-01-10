use anyhow::Result;
use log::{debug, info, trace};
use process_data::ProcessData;
use ron::ser::PrettyConfig;
use std::{
    io::{Read, Write},
    time::Instant,
};

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
    // Initialize logger
    pretty_env_logger::init();

    info!("Starting resources-processes…");

    debug!("Parsing arguments…");
    let args = Args::parse();

    if args.once {
        output(args.ron)?;
        return Ok(());
    }

    debug!("Ready");

    loop {
        let mut buffer = [0; 1];

        std::io::stdin().read_exact(&mut buffer)?;
        trace!("Received character, initiating scan…");

        output(args.ron)?;
    }
}

fn output(ron: bool) -> Result<()> {
    let start = Instant::now();

    trace!("Gathering process data…");
    let data = ProcessData::all_process_data()?;

    let elapsed = start.elapsed();
    trace!(
        "Gathered data for {} processes within {elapsed:.2?}",
        data.len()
    );

    let encoded = if ron {
        trace!("Encoding process data using ron (Resources will not be able to read this!)…");
        ron::ser::to_string_pretty(&data, PrettyConfig::default())?
            .as_bytes()
            .to_vec()
    } else {
        trace!("Encoding process data using rmp…");
        rmp_serde::to_vec(&data)?
    };

    let len_byte_array = encoded.len().to_le_bytes();

    trace!("Preparing stdout…");
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    trace!("Sending content length ({})…", encoded.len());
    handle.write_all(&len_byte_array)?;

    trace!("Sending content…");
    handle.write_all(&encoded)?;

    trace!("Flushing…");
    handle.flush()?;
    Ok(())
}
