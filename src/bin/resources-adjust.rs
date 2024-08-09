use std::{env, path::PathBuf};

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

                adjust(pid, nice, &cpu_set);

                // find tasks that belong to this process
                let tasks_path = PathBuf::from("/proc/").join(pid.to_string()).join("task");
                for entry in std::fs::read_dir(tasks_path).unwrap() {
                    if let Ok(entry) = entry {
                        let thread_id = entry.file_name().to_string_lossy().parse().unwrap();

                        adjust(thread_id, nice, &cpu_set);
                    }
                }

                std::process::exit(0)
            }
        }
    }
    std::process::exit(255);
}

fn adjust(id: i32, nice: i32, cpu_set: &CpuSet) {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, id as u32, nice);
    };

    let error = std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_default();

    if error != 0 {
        std::process::exit(error)
    }

    let _ = sched_setaffinity(Pid::from_raw(id), &cpu_set);
}
