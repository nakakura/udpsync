use std::{env, io};
use std::net::SocketAddr;

use future;
use futures::{Future, Poll, Stream};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::{ Core, Handle };
use tokio_core::net::UdpCodec;

pub struct LineCodec;

impl UdpCodec for LineCodec {
    type In = (SocketAddr, Vec<u8>);
    type Out = (SocketAddr, Vec<u8>);

    fn decode(&mut self, addr: &SocketAddr, buf: &[u8]) -> io::Result<Self::In> {
        Ok((*addr, buf.to_vec()))
    }

    fn encode(&mut self, (addr, buf): Self::Out, into: &mut Vec<u8>) -> SocketAddr {
        into.extend(buf);
        addr
    }
}

pub fn run() {
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let a = UdpSocket::bind(&addr, &handle).unwrap();
    println!("{:?}", a.local_addr().unwrap());

    let (a_sink, a_stream) = a.framed(LineCodec).split();
    let a = a_stream.map_err(|_| ()).fold(0u8, |sum: u8, (addr, x): (SocketAddr, Vec<u8>)| {
        println!("{:?}, {:?}", addr, x);
        Ok(sum)
    });
    drop(core.run(a));
}
