#![feature(extract_if)]
#![feature(hash_extract_if)]
#![feature(let_chains)]
#![feature(never_type)]
#![feature(exit_status_error)]
#![feature(once_cell_try)]
// Very annoying for GObjects just impl Default when you need it
#![allow(clippy::new_without_default)]

pub mod application;
pub mod config;
pub mod gui;
pub mod i18n;
pub mod ui;
pub mod utils;
