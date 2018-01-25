use std::{env, io};
use std::net::SocketAddr;
use std::thread;

use future;
use futures::{Future, Poll, Stream};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::{ Core, Handle };
use tokio_core::net::UdpCodec;
use futures::sync::mpsc;

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

pub fn run(port: u16) {
    let _ = thread::spawn(move || {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();

        let a = UdpSocket::bind(&addr, &handle).unwrap();
        println!("{:?}", a.local_addr().unwrap());

        let (_, a_stream) = a.framed(LineCodec).split();
        let a = a_stream.map_err(|_| ()).fold(0u8, |sum: u8, (addr, x): (SocketAddr, Vec<u8>)| {
            println!("recv {:?}, {:?}", x, Utc::now());
            Ok(sum)
        });
        drop(core.run(a));
    });
}

use futures::Sink;

use chrono::prelude::*;

pub fn sender(port: u16, rx: mpsc::Receiver<(SocketAddr, Vec<u8>)>) {
    let remote_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let _ = thread::spawn(move || {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let local_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let a = UdpSocket::bind(&local_addr, &handle).unwrap();
        let (a_sink, a_stream) = a.framed(LineCodec).split();
        println!("target {:?}", remote_addr);


        use std;
        let sender = a_sink.sink_map_err(|e| {
            eprintln!("err");
        }).send_all(rx);
        //handle.spawn(sender.then(|_| Ok(())));
        drop(core.run(sender));
    });
}