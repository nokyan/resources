#![feature(drain_filter)]
#![feature(hash_drain_filter)]
#![feature(let_chains)]
#![feature(never_type)]
// Very annoying for GObjects just impl Default when you need it
#![allow(clippy::new_without_default)]

pub mod application;
pub mod config;
pub mod gui;
pub mod i18n;
pub mod ui;
pub mod utils;
