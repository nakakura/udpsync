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

#[no_mangle]
#[derive(Debug)]
struct CPacket
{
    ts: u32,
    pts: u64,
}

pub fn ts_to_time(diff: u64) -> f64 {
    diff as f64 / 90f64
}

unsafe fn cpacket_to_bytes<'a>(ptr: CPacket) -> [u8;16] {
    use std::slice;
    use std::mem;

    unsafe {
        mem::transmute::<CPacket, [u8;16]>(ptr)
    }
}