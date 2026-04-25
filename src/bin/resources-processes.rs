use anyhow::Result;
use log::{debug, info, trace, warn};
use process_data::{ProcessData, cache::ProcessDataCache};
use ron::ser::PrettyConfig;
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::LazyLock,
    time::Instant,
};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Run once and then exit (debugging only)
    #[arg(short, long, default_value_t = false)]
    once: bool,

    /// Use Rusty Object Notation (debugging only)
    #[arg(short, long, default_value_t = false)]
    ron: bool,

    /// Disable fdinfo caching
    #[arg(short = 'f', long, default_value_t = false)]
    disable_fdinfo_caching: bool,
}

static PROCFS: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("/proc"));

fn main() -> Result<()> {
    // Initialize logger
    pretty_env_logger::init();

    info!("Starting resources-processes…");

    debug!("Parsing arguments…");
    let args = Args::parse();

    if args.ron {
        warn!(
            "Output in Rusty Object Notation instead of binary is enable. You should use this *only for debugging resources-processes itself*, Resources will not be able to parse this output!"
        );
    }

    if args.once {
        warn!(
            "One-time run instead of interactive run is enabled. You should use this *only for debugging resources-processes itself*, Resources needs to be able to interact with resources-processes!"
        )
    }

    let mut cache = if args.disable_fdinfo_caching {
        ProcessDataCache::new_no_fdinfo_cache()
    } else {
        ProcessDataCache::new()
    };

    if args.once {
        output(args.ron, &mut cache)?;
        return Ok(());
    }

    debug!("Ready");

    loop {
        let mut buffer = [0; 1];

        std::io::stdin().read_exact(&mut buffer)?;
        trace!("Received character, initiating scan…");

        output(args.ron, &mut cache)?;
    }
}

fn output(ron: bool, cache: &mut ProcessDataCache) -> Result<()> {
    let start = Instant::now();

    trace!("Gathering process data…");
    let data = ProcessData::all_from_procfs(&PROCFS, cache)?;

    let elapsed = start.elapsed();
    trace!(
        "Gathered data for {} processes within {elapsed:.2?}",
        data.len()
    );

    let encoded = if ron {
        trace!("Encoding process data using ron…");
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
