use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;
use tokio_core::net::UdpCodec;
use futures::{Future, Stream};
use futures::sync::mpsc;
use futures::Sink;

use std::thread;
use std::net::SocketAddr;

use rtp;
use udp;

pub fn recv_rtp(local_sock:SocketAddr, remote_sock: SocketAddr) -> thread::JoinHandle<()> {
    //recv rtp
    let (rtp_receiver_tx, rtp_receiver_rx) = mpsc::channel::<Vec<u8>>(5000);
    udp::receiver(local_sock, rtp_receiver_tx);

    //send timestamp,pts
    let (ts_sender_tx, ts_sender_rx) = mpsc::channel::<(SocketAddr, Vec<u8>)>(5000);
    udp::sender(ts_sender_rx);

    thread::spawn(move || {
        let mut core = Core::new().unwrap();
        let r = rtp_receiver_rx.map(|buf| {
            unsafe {
                rtp::timestamp(buf.as_ptr())
            }
        }).fold((ts_sender_tx, 0, None), |(sender, sum, last_ts), ts: u32| {
            let mut diff = 0;
            if let Some(last_ts) = last_ts {
                diff = ts - last_ts;
            }

            let sum = sum + ts_to_time(diff) as u64;
            use chrono::*;
            println!("diff {:?} {:?}", sum, Utc::now());
            let packet = CPacket {
                ts: ts,
                pts: sum,
            };
            let data: Vec<u8> = unsafe {
                cpacket_to_bytes(packet).to_vec()
            };

            let sender = sender.send((remote_sock, data)).wait().unwrap();
            Ok((sender, sum, Some(ts)))
        });
        let _ = core.run(r);
    })
}

#[no_mangle]
#[derive(Debug)]
struct CPacket
{
    ts: u32,
    pts: u64,
}

pub fn ts_to_time(diff: u32) -> f64 {
    (diff as u64 * 1000u64 * 10u64) as f64 / 9f64
}

unsafe fn cpacket_to_bytes<'a>(ptr: CPacket) -> [u8;16] {
    use std::slice;
    use std::mem;

    unsafe {
        mem::transmute::<CPacket, [u8;16]>(ptr)
    }
}