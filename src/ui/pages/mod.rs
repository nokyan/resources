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
