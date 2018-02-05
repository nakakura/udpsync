#![feature(drain_filter)] extern crate env_logger;
extern crate futures;
extern crate chrono;
extern crate tokio_io;
extern crate tokio_core;

extern crate udpsync;

use chrono::*;
use futures::*;
use futures::sync::mpsc;
use futures::Sink;
use tokio_core::reactor::Core;
use tokio_core::net::UdpCodec;
use futures::{Future, Stream};

use std::net::SocketAddr;
use std::thread;

use udpsync::haptic_data::HapticData;
use udpsync::buffer::PlayTimingGap;

fn main() {
    //gst-mock
    let local_sock: SocketAddr = format!("127.0.0.1:{}", 30000).parse().unwrap();
    let remote_sock: SocketAddr = format!("127.0.0.1:{}", 60000).parse().unwrap();
    let th_gst = udpsync::gstreamer_mock::recv_rtp(local_sock, remote_sock);

    //recv rtp from sender and redirect it to gstreamer
    let bind_addr_rtp: SocketAddr = format!("127.0.0.1:{}", 20000).parse().unwrap();
    let target_addr_rtp: SocketAddr = format!("127.0.0.1:{}", 30000).parse().unwrap();
    let (recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_rtp_1 = udpsync::udp::receiver(bind_addr_rtp, recv_rtp_tx);
    let th_rtp_2 = udpsync::udp::sender(recv_rtp_rx.map(move |x| (target_addr_rtp, x)));

    //recv hapt data from sender and redirect it to haptic player through ring buffer
    let bind_addr_hapt: SocketAddr = format!("127.0.0.1:{}", 20001).parse().unwrap();
    let target_addr_hapt: SocketAddr = format!("127.0.0.1:{}", 30001).parse().unwrap();
    let (recv_hapt_tx, recv_hapt_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_hapt_1 = udpsync::udp::receiver(bind_addr_hapt, recv_hapt_tx);
    let th_hapt_2 = thread::spawn(|| {
        let mut core = Core::new().unwrap();
        let r = recv_hapt_rx.map(|x| {
            let data = HapticData::decode(&x);
            (data, None)
        }).for_each(|x: (HapticData, Option<PlayTimingGap>)| {
            println!("{:?}", x.0.timestamp);
            Ok(())
        });
        let _ = core.run(r);
    });

    //recv data from gst_mock
    let bind_addr_rtp: SocketAddr = format!("127.0.0.1:{}", 60000).parse().unwrap();
    let (recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_rtp_1 = udpsync::udp::receiver(bind_addr_rtp, recv_rtp_tx);
    let _ = thread::spawn(|| {
        let mut core = Core::new().unwrap();
        let r = recv_rtp_rx.fold((None, None), |(initial_time_opt, initial_ts_opt): (Option<DateTime<Utc>>, Option<u32>), x| {
            let data_ptr: *const u8 = x.as_ptr();
            let header_ptr: *const CPacket = data_ptr as *const _;
            let padding_ref: &CPacket = unsafe { &*header_ptr };


            let initial_time = initial_time_opt.unwrap_or(Utc::now());
            let initial_ts = initial_ts_opt.unwrap_or(padding_ref.ts);

            let diff = udpsync::gstreamer_mock::ts_to_time(padding_ref.ts - initial_ts) as i64;
            let source_timestamp = initial_time + Duration::nanoseconds(diff as i64);
            let play_time = initial_time + Duration::nanoseconds(padding_ref.pts as i64);

            //println!("pts {:?}, {:?}", source_timestamp, play_time);
            Ok((Some(initial_time), Some(initial_ts)))
        });
        let _ = core.run(r);
    }).join();

    let _ = th_gst.join();
    let _ = th_rtp_1.join();
    let _ = th_rtp_2.join();
    let _ = th_hapt_1.join();
    let _ = th_hapt_2.join();
}

#[no_mangle]
#[derive(Debug)]
struct CPacket
{
    ts: u32,
    pts: u64,
}
