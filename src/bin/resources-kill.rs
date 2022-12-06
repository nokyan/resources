#![feature(let_chains)]

use std::{env, process::Command};

fn main() {
    if let Some(arg) = env::args().nth(1) && let Some(pid) = env::args().nth(2){
        let ret_value = Command::new("kill").args(["-s", &arg, &pid]).output().unwrap().status.code().unwrap_or(1);
        std::process::exit(ret_value);
    };
    std::process::exit(1);
}
