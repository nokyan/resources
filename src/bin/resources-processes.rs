use anyhow::Result;
use log::{debug, info, trace};
use process_data::ProcessData;
use ron::ser::PrettyConfig;
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    path::PathBuf,
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

    let mut non_gpu_fdinfos = HashSet::new();
    let mut non_npu_fdinfos = HashSet::new();
    let mut symlink_cache = HashMap::new();

    if args.once {
        output(
            args.ron,
            &mut non_gpu_fdinfos,
            &mut non_npu_fdinfos,
            &mut symlink_cache,
        )?;
        return Ok(());
    }

    debug!("Ready");

    loop {
        let mut buffer = [0; 1];

        std::io::stdin().read_exact(&mut buffer)?;
        trace!("Received character, initiating scan…");

        output(
            args.ron,
            &mut non_gpu_fdinfos,
            &mut non_npu_fdinfos,
            &mut symlink_cache,
        )?;
    }
}

fn output(
    ron: bool,
    non_gpu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
    non_npu_fdinfos: &mut HashSet<(libc::pid_t, usize)>,
    symlink_cache: &mut HashMap<(libc::pid_t, usize), PathBuf>,
) -> Result<()> {
    let start = Instant::now();

    trace!("Gathering process data…");
    let data = ProcessData::all_process_data(non_gpu_fdinfos, non_npu_fdinfos, symlink_cache)?;

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
