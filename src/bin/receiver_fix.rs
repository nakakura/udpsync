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
    let redirect_rtp_th = redirect_rtp();

    let (recv_pts_pair_tx, recv_pts_pair_rx) = mpsc::channel::<(Option<HapticData>, Option<PlayTimingGap>)>(5000);
    let recv_pts_pair_th = recv_pts_pair(recv_pts_pair_tx);

    let (recv_hapt_tx, recv_hapt_rx) = mpsc::channel::<Vec<u8>>(5000);
    let recv_hapt_th = recv_hapt(recv_hapt_tx);

    let hapt_src_rx = recv_pts_pair_rx.select(recv_hapt_rx.map(|value| {
        let mut data = HapticData::decode(&value);
        (Some(data), None)
    }));

    let (sync_hapt_tx, sync_hapt_rx) = mpsc::channel::<PlayDataAndTime>(5000);

    let sync_hapt_th = thread::spawn(move || {
        let mut core = Core::new().unwrap();
        let r = hapt_src_rx.fold((sync_hapt_tx, vec!(), vec!()), check_time);
        let _ = core.run(r);
    });

    let (redirect_hapt_tx, redirect_hapt_rx) = mpsc::channel::<Vec<u8>>(5000);
    let redirect_hapt_tx = redirect_hapt_tx.with_flat_map(|values: Vec<Vec<u8>>| {
        stream::iter_ok(values.into_iter().map(move |value| {
            value
        }))
    });

    let (send_hapt_tx, send_hapt_rx) = mpsc::channel::<Vec<u8>>(5000);
    let redirect_hapt_th = thread::spawn(move || {
        let mut core = Core::new().unwrap();

        let r = sync_hapt_rx.fold(redirect_hapt_tx, |sender, PlayDataAndTime((vec, time))| {
            while time > Utc::now() {
                thread::sleep_ms(1);
            }
            let sender = sender.send(vec).wait().unwrap();
            Ok(sender)
        });
        let _ = core.run(r);
    });
    let target: SocketAddr = format!("127.0.0.1:{}", 30001).parse().unwrap();
    let th_redirect = udpsync::udp::sender(redirect_hapt_rx.map(move |x| {
        (target, x)
    }));

    let _ = recv_hapt_th.join();
    let _ = recv_pts_pair_th.join();
    let _ = redirect_rtp_th.join();
    let _ = sync_hapt_th.join();
}

fn recv_hapt(output: mpsc::Sender<Vec<u8>>) -> thread::JoinHandle<()> {
    thread::spawn(|| {
        let hapt_src_port: SocketAddr = format!("0.0.0.0:{}", 20001).parse().unwrap();
        let th_hapt_1 = udpsync::udp::receiver(hapt_src_port, output);
        let _ = th_hapt_1.join();
    })
}

fn redirect_rtp() -> thread::JoinHandle<()> {
    thread::spawn(|| {
        //recv rtp from sender and redirect it to gstreamer
        let rtp_src_port: SocketAddr = format!("0.0.0.0:{}", 20000).parse().unwrap();
        let rtp_target_port: SocketAddr = format!("127.0.0.1:{}", 7100).parse().unwrap();
        let (redirect_rtp_tx, redirect_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
        let th_rtp_1 = udpsync::udp::receiver(rtp_src_port, redirect_rtp_tx);
        let th_rtp_2 = udpsync::udp::sender(redirect_rtp_rx.map(move |x| {
            if first_timestamp() == 0 {
                let ts = unsafe {
                    udpsync::rtp::timestamp(x.as_ptr()) as u64
                };
                set_first_timestamp(ts);
            }
            (rtp_target_port, x)
        }));
        let _ = th_rtp_1.join();
        let _ = th_rtp_2.join();
    })
}

fn recv_pts_pair(output: mpsc::Sender<(Option<HapticData>, Option<PlayTimingGap>)>) -> thread::JoinHandle<()> {
    //recv ts, pts from gstreamer
    let pts_pair_src_port: SocketAddr = format!("0.0.0.0:{}", 60000).parse().unwrap();
    let (recv_pts_pair_tx, recv_pts_pair_rx) = mpsc::channel::<Vec<u8>>(5000);
    let th_rtp_1 = udpsync::udp::receiver(pts_pair_src_port, recv_pts_pair_tx);
    thread::spawn(|| {
        let mut core = Core::new().unwrap();
        let r = recv_pts_pair_rx.fold((output, None, None, None, None), |(sender, initial_time_opt, prev_ts_opt, total_rtp_time_gap_opt, initial_pts_opt), x| {
            let data_ptr: *const u8 = x.as_ptr();
            let header_ptr: *const CPacket = data_ptr as *const _;
            let padding_ref: &CPacket = unsafe { &*header_ptr };

            let current_rtp_time_gap: i64 = total_rtp_time_gap_opt.unwrap_or(0);
            let prev_ts = prev_ts_opt.unwrap_or(first_timestamp());
            let ts_diff: u64 = (padding_ref.ts as u64 + std::u32::MAX as u64 - prev_ts) % std::u32::MAX as u64;
            let rtp_time_diff = udpsync::gstreamer_mock::ts_to_time(ts_diff) as i64;
            let total_rtp_time_gap = current_rtp_time_gap + rtp_time_diff;
            let source_position = Utc.timestamp(0, 0) + Duration::milliseconds(total_rtp_time_gap);

            let initial_time = initial_time_opt.unwrap_or(Utc::now());
            let initial_pts = initial_pts_opt.unwrap_or(padding_ref.pts);
            let play_time = initial_time + Duration::nanoseconds((padding_ref.pts - initial_pts) as i64);

            let sender = sender.send(
                (
                    None,
                    Some(PlayTimingGap((source_position, play_time)))
                )
            ).wait().unwrap();

            Ok((sender, Some(initial_time), Some(padding_ref.ts as u64), Some(total_rtp_time_gap as u64), Some(initial_pts)))
        });
        let _ = core.run(r);
    })
}

#[no_mangle]
#[derive(Debug)]
struct CPacket
{
    ts: u64,
    pts: u64,
}