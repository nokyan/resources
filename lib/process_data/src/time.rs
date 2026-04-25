use std::time::SystemTime;

pub fn unix_as_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system time is before UNIX epoch")
        .as_millis() as u64
}

pub fn unix_as_secs_f64() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system time is before UNIX epoch")
        .as_secs_f64()
}
