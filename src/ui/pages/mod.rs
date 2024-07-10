pub mod applications;
pub mod battery;
pub mod cpu;
pub mod drive;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod processes;

pub const APPLICATIONS_PRIMARY_ORD: u32 = 0;
pub const PROCESSES_PRIMARY_ORD: u32 = 1;
pub const CPU_PRIMARY_ORD: u32 = 2;
pub const MEMORY_PRIMARY_ORD: u32 = 3;
pub const GPU_PRIMARY_ORD: u32 = 4;
pub const DRIVE_PRIMARY_ORD: u32 = 5;
pub const NETWORK_PRIMARY_ORD: u32 = 6;
pub const BATTERY_PRIMARY_ORD: u32 = 7;
