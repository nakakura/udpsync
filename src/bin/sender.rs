#![feature(drain_filter)]
extern crate futures;
extern crate chrono;
extern crate tokio_io;

extern crate udpsync;

use futures::*;
use futures::sync::mpsc;
use chrono::*;

use std::net::SocketAddr;

fn main() {
    //recv and redirect rtp from gstreamer
    let bind_addr_rtp: SocketAddr = format!("0.0.0.0:{}", 10000).parse().unwrap();
    let target_addr_rtp: SocketAddr = format!("192.168.20.101:{}", 20000).parse().unwrap();
    let (recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_rtp_1 = udpsync::udp::receiver(bind_addr_rtp, recv_rtp_tx);
    let th_rtp_2 = udpsync::udp::sender(recv_rtp_rx.map(move |x| (target_addr_rtp, x)));

    //recv and redirect data from hapt sensor
    let bind_addr_hapt: SocketAddr = format!("0.0.0.0:{}", 10001).parse().unwrap();
    let target_addr_hapt: SocketAddr = format!("192.168.20.101:{}", 20001).parse().unwrap();
    let (recv_hapt_tx, recv_hapt_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_hapt_1 = udpsync::udp::receiver(bind_addr_hapt, recv_hapt_tx);
    let th_hapt_2 = udpsync::udp::sender(recv_hapt_rx.map(move |x| {
        let data = udpsync::haptic_data::HapticData::new(x, Utc::now());
        let bin = data.encode().unwrap();
        (target_addr_hapt, bin)
    }));

    let _ = th_rtp_1.join();
    let _ = th_rtp_2.join();
    let _ = th_hapt_1.join();
    let _ = th_hapt_2.join();
}
