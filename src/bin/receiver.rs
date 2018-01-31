#![feature(drain_filter)]
extern crate env_logger;
extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate chrono;
extern crate tokio_io;

extern crate udpsync;

use futures::*;
use futures::sync::mpsc;
use tokio_core::reactor::Core;

use std::thread;
use std::net::SocketAddr;

fn main() {
    //recv rtp from gstreamer
    let (mut recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    udpsync::udp::receiver(60000, recv_rtp_tx);

    let th = thread::spawn(|| {
        let mut core = Core::new().unwrap();
        let r = recv_rtp_rx.map(|buf| {
            (0, buf)
        }).for_each(|x| {
            println!("{:?}", x);
            Ok(())
        });
        core.run(r);
    });

    let _ = th.join();
}