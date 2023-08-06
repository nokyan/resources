#![feature(let_chains)]

use std::env;

use nix::{sys::signal, unistd::Pid};

fn main() {
    if let Some(arg) = env::args().nth(1) && let Some(pid) = env::args().nth(2).and_then(|s| s.trim().parse().ok()) {
        let signal = match arg.as_str() {
            "STOP" => signal::Signal::SIGSTOP,
            "CONT" => signal::Signal::SIGCONT,
            "TERM" => signal::Signal::SIGTERM,
            "KILL" => signal::Signal::SIGKILL,
            _ => std::process::exit(254),
        };
        let result = signal::kill(Pid::from_raw(pid), Some(signal));
        if let Err(err) = result {
            match err {
                nix::errno::Errno::UnknownErrno => std::process::exit(253),
                _ => std::process::exit(err as i32),
            };
        }
        std::process::exit(0);
    };
    std::process::exit(255);
}
