#![feature(extract_if)]
#![feature(hash_extract_if)]
#![feature(let_chains)]
#![feature(never_type)]
// Very annoying for GObjects just impl Default when you need it
#![allow(clippy::new_without_default)]
#![feature(exit_status_error)]

pub mod application;
pub mod config;
pub mod gui;
pub mod i18n;
pub mod ui;
pub mod utils;
