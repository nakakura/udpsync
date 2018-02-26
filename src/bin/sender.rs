#![feature(drain_filter)]
extern crate futures;
extern crate chrono;
extern crate tokio_io;

extern crate udpsync;

use futures::*;
use futures::sync::mpsc;
use chrono::*;

use std::net::SocketAddr;

#[derive(Debug)]
struct TS
{
    pts: u64,
}

fn main() {
    let target_addr = "192.168.20.101";
    //recv and redirect rtp from gstreamer
    let bind_addr_rtp: SocketAddr = format!("0.0.0.0:{}", 10000).parse().unwrap();
    let target_addr_rtp: SocketAddr = format!("{}:{}", target_addr, 20000).parse().unwrap();
    let (recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_rtp_1 = udpsync::udp::receiver(bind_addr_rtp, recv_rtp_tx);
    let th_rtp_2 = udpsync::udp::sender(recv_rtp_rx.map(move |x| (target_addr_rtp, x)));

    //recv and redirect data from hapt sensor
    let bind_addr_hapt: SocketAddr = format!("0.0.0.0:{}", 10001).parse().unwrap();
    let target_addr_hapt: SocketAddr = format!("{}:{}", target_addr, 20001).parse().unwrap();
    let (recv_hapt_tx, recv_hapt_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_hapt_1 = udpsync::udp::receiver(bind_addr_hapt, recv_hapt_tx);
    let th_hapt_2 = udpsync::udp::sender(recv_hapt_rx.map(move |x| {
        let data_ptr: *const u8 = x.as_ptr();
        let header_ptr: *const TS = data_ptr as *const _;
        let padding_ref: &TS = unsafe { &*header_ptr };
        let duration = Duration::nanoseconds(padding_ref.pts as i64);
        let data = udpsync::haptic_data::HapticData::new(x, Utc.timestamp(0, 0) + duration);
        let bin = data.encode().unwrap();
        (target_addr_hapt, bin)
    }));

    let _ = th_rtp_1.join();
    let _ = th_rtp_2.join();
    let _ = th_hapt_1.join();
    let _ = th_hapt_2.join();
}
