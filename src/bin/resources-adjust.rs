use std::env;

use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};

fn main() {
    if let Some(pid) = env::args().nth(1).and_then(|s| s.trim().parse().ok()) {
        if let Some(nice) = env::args().nth(2).and_then(|s| s.trim().parse().ok()) {
            if let Some(mask) = env::args().nth(3) {
                let mut cpu_set = CpuSet::new();

                for (i, c) in mask.chars().enumerate() {
                    if c == '1' {
                        cpu_set.set(i).unwrap_or_default();
                    }
                }

                let _ = sched_setaffinity(Pid::from_raw(pid), &cpu_set);

                unsafe {
                    libc::setpriority(libc::PRIO_PROCESS, pid as u32, nice);
                };

                let error = std::io::Error::last_os_error()
                    .raw_os_error()
                    .unwrap_or_default();

                std::process::exit(error)
            }
        }
    }
    std::process::exit(255);
}
