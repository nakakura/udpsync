#![feature(drain_filter)]
extern crate env_logger;
extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate chrono;
extern crate tokio_io;
extern crate bincode;
extern crate rustc_serialize;

pub mod udp;
pub mod rtp;
pub mod haptic_data;
pub mod buffer;

