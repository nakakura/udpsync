#![feature(drain_filter)]
extern crate futures;
extern crate chrono;
extern crate tokio_io;
extern crate tokio_core;

extern crate udpsync;
#[macro_use] extern crate lazy_static;

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
use udpsync::buffer::*;

use std::sync::RwLock;

lazy_static! {
    pub static ref FIRST_TIMESTAMP: RwLock<u64> = {
        RwLock::new(0)
    };
}

fn first_timestamp() -> u64 {
    *FIRST_TIMESTAMP.read().unwrap()
}

fn set_first_timestamp(ts: u64) {
    *FIRST_TIMESTAMP.write().unwrap() = ts;
}

fn main() {
    let th_key = udpsync::keyboard::get_keyboard(udpsync::buffer::set_offset);

    let (tx, rx) = mpsc::channel::<Vec<u8>>(5000);
    let tx = tx.with_flat_map(|values: Vec<Vec<u8>>| {
        stream::iter_ok(values.into_iter().map(move |value| {
            value
        }))
    });

    let (recv_ts_tx, recv_ts_rx) = mpsc::channel::<(Option<HapticData>, Option<PlayTimingGap>)>(5000);
    let (redirect_hapt_tx, redirect_hapt_rx) = mpsc::channel::<(Option<HapticData>, Option<PlayTimingGap>)>(5000);
    let hapt_play_rx = recv_ts_rx.select(redirect_hapt_rx);
    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);
    let th_redirect1 = thread::spawn(move || {
        let mut core = Core::new().unwrap();

        let r = hapt_play_rx.fold((sender, vec!(), vec!()), check_time);
        let _ = core.run(r);
    });
    let th_redirect2 = thread::spawn(move || {
        let mut core = Core::new().unwrap();

        let r = receiver.fold(tx, |sender, PlayDataAndTime((vec, time))| {
            while time > Utc::now() {
                thread::sleep_ms(1);
            }
            let sender = sender.send(vec).wait().unwrap();
            Ok(sender)
        });
        let _ = core.run(r);
    });

    let target: SocketAddr = format!("127.0.0.1:{}", 30001).parse().unwrap();
    let th_redirect = udpsync::udp::sender(rx.map(move |x| {
        (target, x)
    }));

    //recv rtp from sender and redirect it to gstreamer
    let bind_addr_rtp: SocketAddr = format!("0.0.0.0:{}", 20000).parse().unwrap();
    let target_addr_rtp: SocketAddr = format!("127.0.0.1:{}", 7100).parse().unwrap();
    let (recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_rtp_1 = udpsync::udp::receiver(bind_addr_rtp, recv_rtp_tx);
    let th_rtp_2 = udpsync::udp::sender(recv_rtp_rx.map(move |x|{
        if first_timestamp() == 0 {
            let ts = unsafe {
                udpsync::rtp::timestamp(x.as_ptr()) as u64
            };
            set_first_timestamp(ts);
        }
        (target_addr_rtp, x)
    }));

    //recv hapt data from sender and redirect it to haptic player through ring buffer
    let bind_addr_hapt: SocketAddr = format!("0.0.0.0:{}", 20001).parse().unwrap();
    let (recv_hapt_tx, recv_hapt_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_hapt_1 = udpsync::udp::receiver(bind_addr_hapt, recv_hapt_tx);
    let th_hapt_2 = thread::spawn(|| {
        let mut core = Core::new().unwrap();
        let r = recv_hapt_rx.fold(redirect_hapt_tx, |sender, x| {
            //let initial_time = initial_time_opt.unwrap_or(Utc::now());
            let mut data = HapticData::decode(&x);
            //let initial_ts = initial_ts_opt.unwrap_or(data.timestamp);
            //data.timestamp = initial_time + (data.timestamp.signed_duration_since(initial_ts));
            let sender = sender.send((Some(data), None)).wait().unwrap();
            Ok(sender)
        });
        let _ = core.run(r);
    });

    //recv ts, pts from gstreamer
    let bind_addr_rtp: SocketAddr = format!("0.0.0.0:{}", 60000).parse().unwrap();
    let (recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_rtp_1 = udpsync::udp::receiver(bind_addr_rtp, recv_rtp_tx);
    let _ = thread::spawn(|| {
        let mut core = Core::new().unwrap();
        let r = recv_rtp_rx.fold((recv_ts_tx, None, None, None, None), |(sender, initial_time_opt, prev_ts_opt, ts_diff_sum_opt, initial_pts_opt), x| {
            let data_ptr: *const u8 = x.as_ptr();
            let header_ptr: *const CPacket = data_ptr as *const _;
            let padding_ref: &CPacket = unsafe { &*header_ptr };
            let initial_time = initial_time_opt.unwrap_or(Utc::now());
            let initial_pts = initial_pts_opt.unwrap_or(padding_ref.pts);
            let prev_ts = prev_ts_opt.unwrap_or(first_timestamp());
            let ts_diff_sum: u64 = ts_diff_sum_opt.unwrap_or(0);

            let ts_diff: u64 = (padding_ref.ts as u64 + std::u32::MAX as u64 - prev_ts) % std::u32::MAX as u64;
            let ts_diff_sum = ts_diff_sum + ts_diff;
            let diff = udpsync::gstreamer_mock::ts_to_time(ts_diff_sum) as i64;
            let source_timestamp = Utc.timestamp(0, 0) + Duration::milliseconds(diff);
            let play_time = initial_time + Duration::nanoseconds((padding_ref.pts - initial_pts) as i64);

            let now = Utc::now();
            //println!("play_time {:?} {:?}", now.signed_duration_since(source_timestamp).num_milliseconds(), now.signed_duration_since(play_time).num_milliseconds());
            let sender = sender.send(
                (
                    None,
                    Some(PlayTimingGap((source_timestamp, play_time)))
                )
            ).wait().unwrap();

            Ok((sender, Some(initial_time), Some(padding_ref.ts as u64), Some(ts_diff_sum), Some(initial_pts)))
        });
        let _ = core.run(r);
    }).join();

    let _ = th_key.join();
    let _ = th_rtp_1.join();
    let _ = th_rtp_2.join();
    let _ = th_hapt_1.join();
    let _ = th_hapt_2.join();
}

#[no_mangle]
#[derive(Debug)]
struct CPacket
{
    ts: u64,
    pts: u64,
}