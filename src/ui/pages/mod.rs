use std::{collections::HashMap, sync::LazyLock};

use process_data::Niceness;

use crate::i18n::i18n;

pub mod applications;
pub mod battery;
pub mod cpu;
pub mod drive;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod processes;

const APPLICATIONS_PRIMARY_ORD: u32 = 0;
const PROCESSES_PRIMARY_ORD: u32 = 1;
const CPU_PRIMARY_ORD: u32 = 2;
const MEMORY_PRIMARY_ORD: u32 = 3;
const GPU_PRIMARY_ORD: u32 = 4;
const DRIVE_PRIMARY_ORD: u32 = 5;
const NETWORK_PRIMARY_ORD: u32 = 6;
const BATTERY_PRIMARY_ORD: u32 = 7;

pub static NICE_TO_LABEL: LazyLock<HashMap<Niceness, (String, u32)>> = LazyLock::new(|| {
    let mut hash_map = HashMap::new();

    for i in -20..=-8 {
        hash_map.insert(Niceness::try_new(i).unwrap(), (i18n("Very High"), 0));
    }

    for i in -7..=-3 {
        hash_map.insert(Niceness::try_new(i).unwrap(), (i18n("High"), 1));
    }

    for i in -2..=2 {
        hash_map.insert(Niceness::try_new(i).unwrap(), (i18n("Normal"), 2));
    }

    for i in 3..=6 {
        hash_map.insert(Niceness::try_new(i).unwrap(), (i18n("Low"), 3));
    }

    for i in 7..=19 {
        hash_map.insert(Niceness::try_new(i).unwrap(), (i18n("Very Low"), 4));
    }

    hash_map
});
