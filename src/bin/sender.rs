#![feature(drain_filter)] extern crate env_logger;
extern crate futures;
extern crate chrono;
extern crate tokio_io;

extern crate udpsync;

use futures::*;
use futures::sync::mpsc;

use std::net::SocketAddr;

fn main() {
    //recv and redirect rtp from gstreamer
    let bind_addr_rtp: SocketAddr = format!("127.0.0.1:{}", 60000).parse().unwrap();
    let target_addr_rtp: SocketAddr = format!("127.0.0.1:{}", 61234).parse().unwrap();
    let (recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_rtp_1 = udpsync::udp::receiver(bind_addr_rtp, recv_rtp_tx);
    let th_rtp_2 = udpsync::udp::send_all(recv_rtp_rx.map(move |x| (target_addr_rtp, x)));

    //recv and redirect data from hapt sensor
    let bind_addr_hapt: SocketAddr = format!("127.0.0.1:{}", 10000).parse().unwrap();
    let target_addr_hapt: SocketAddr = format!("127.0.0.1:{}", 20000).parse().unwrap();
    let (recv_hapt_tx, recv_hapt_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_hapt_1 = udpsync::udp::receiver(bind_addr_hapt, recv_hapt_tx);
    let th_hapt_2 = udpsync::udp::send_all(recv_hapt_rx.map(move |x| (target_addr_hapt, x)));

    let _ = th_rtp_1.join();
    let _ = th_rtp_2.join();
    let _ = th_hapt_1.join();
    let _ = th_hapt_2.join();
}
