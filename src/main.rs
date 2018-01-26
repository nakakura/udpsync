#![feature(drain_filter)]
extern crate env_logger;
extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate chrono;
extern crate tokio_io;

pub mod udp;
pub mod rtp;

use chrono::prelude::*;
use chrono::Duration;
use futures::*;
use futures::sync::mpsc;
use tokio_core::reactor::Core;
use tokio_core::net::UdpSocket;

use std::{env, io};
use std::net::SocketAddr;
use std::thread;

use futures::Sink;

fn recv_rtp() {

}

fn main() {
    let remote_addr: SocketAddr = "127.0.0.1:10000".parse().unwrap();
    let (mut tx, rx) = mpsc::channel::<(SocketAddr, Vec<u8>)>(5000);
    let (mut tx_w_sleep, rx_w_sleep) = mpsc::channel::<(DateTime<Utc>, SocketAddr, Vec<Vec<u8>>)>(5000);

    let (mut tx_recv, rx_recv) = mpsc::channel::<Vec<u8>>(5000);
    udp::run(10000, tx_recv);
    udp::sender(10000, rx);

    thread::spawn(|| {
        let mut core = Core::new().unwrap();
        let r = rx_recv.map(|buf| {
            let ts = unsafe {
                rtp::timestamp(buf.as_ptr())
            };
            (ts, buf)
        }).for_each(|x| {
            println!("{:?}", x);
            Ok(())
        });
        core.run(r);
    });

    /*
    thread::spawn(|| {
        let mut core = Core::new().unwrap();
        let tx = tx.with_flat_map(|(addr, values): (SocketAddr, Vec<Vec<u8>>)| {
            stream::iter_ok(values.into_iter().map(move |value| {
                (addr, value)
            }))
        });
        let rx_w_sleep = rx_w_sleep.fold(tx, |sender, (time, addr, value)| {
            println!("send {:?} {:?}", value, time);
            while time > Utc::now() {
                thread::sleep_ms(1);
            }
            Ok(sender.send((addr, value)).wait().unwrap())
        });
        drop(core.run(rx_w_sleep));
    });
*/

    let sec = Duration::milliseconds(500);
    let mut sendtime = Utc::now();
    for i in 0..5 {
        sendtime = sendtime + sec;
        tx_w_sleep = tx_w_sleep.send((sendtime, remote_addr, vec!(vec!(i * 3), vec!(i * 3 + 1), vec!(i * 3 + 2)))).wait().unwrap();
    }

    thread::sleep_ms(100000);
}

#[derive(PartialEq, PartialOrd, Clone, Debug)]
struct HaptAndTimestamp(pub (Vec<u8>, DateTime<Utc>)); //data from haptic receiver
#[derive(PartialEq, PartialOrd, Clone, Debug)]
struct PlayTimingGap(pub (DateTime<Utc>, DateTime<Utc>)); //data from gstreamer
#[derive(PartialEq, PartialOrd, Clone, Debug)]
struct PlayDataAndTime(pub (Vec<Vec<u8>>, DateTime<Utc>)); //data for send

impl PlayDataAndTime {
    pub fn len(&self) -> usize {
        let &PlayDataAndTime((ref vec, ref _time)) = self;
        vec.len()
    }
}

fn extract_playabledata(mut data: Vec<HaptAndTimestamp>, &PlayTimingGap(time): &PlayTimingGap) -> (Vec<HaptAndTimestamp>, PlayDataAndTime) {
    //古すぎるデータは再生せず捨てる
    let _too_old_data = data.drain_filter(|&mut HaptAndTimestamp(ref x)| {
        x.1 < time.0 - Duration::milliseconds(1)
    }).collect::<Vec<_>>();

    //再生データの取り出し
    let playable_data = data.drain_filter(|&mut HaptAndTimestamp(ref x)| {
        x.1 <= time.0 + Duration::microseconds(16666)
    }).map(|HaptAndTimestamp(x)| x.0).collect::<Vec<_>>();

    (data, PlayDataAndTime((playable_data, time.1)))
}

/// 触覚センサから来たタイムスタンプ付きデータと、
/// gstreamerからやってきたRTPタイムスタンプと再生時刻のペアを突き合わせ、再生可能なものをfutureとして吐き出す
/// 但し、触覚センサは送信側ローカルクロック、
/// RTPタイムスタンプと再生時刻のペアはそれぞれ再生開始時点からの相対位置であるので、
/// この関数に入れる前に受信側ローカルクロックへ変換が必要である
/// 送信するときは再生予定時刻を付けて送る
fn check_time(sum: (mpsc::Sender<PlayDataAndTime>, Vec<HaptAndTimestamp>, PlayTimingGap), acc: (Option<HaptAndTimestamp>, Option<PlayTimingGap>)) -> (mpsc::Sender<PlayDataAndTime>, Vec<HaptAndTimestamp>, PlayTimingGap){
    match acc {
        (Some(t), _) => {
            let mut data = sum.1;
            data.push(t);

            let (nonplayable, play_data) = extract_playabledata(data, &sum.2);
            let sender = if play_data.len() > 0 {
                sum.0.send(play_data).wait().unwrap()
            } else {
                sum.0
            };

            (sender, nonplayable, sum.2)
        },
        (_, Some(p)) => {
            let (nonplayable, play_data) = extract_playabledata(sum.1, &p);
            let sender = if play_data.len() > 0 {
                sum.0.send(play_data).wait().unwrap()
            } else {
                sum.0
            };

            (sender, nonplayable, p)
        },
        _ => {
            sum
        }
    }
}

fn udpsync() {
    let utc: DateTime<Utc> = Utc::now();
    let (data_tx, data_rx) = mpsc::channel::<(Option<HaptAndTimestamp>, Option<PlayTimingGap>)>(5000);
    let (time_tx, time_rx) = mpsc::channel::<(Option<HaptAndTimestamp>, Option<PlayTimingGap>)>(5000);
    let rx = data_rx.select(time_rx);
}

#[test]
fn test_extract_playable_data() {
    //0ms経過時のデータ
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HaptAndTimestamp((data0.clone(), time0));

    //1ms経過時のデータ
    let data1 = vec!(1u8, 10u8);
    let time1 = Utc.timestamp(0, 1 * 1000 * 1000);
    let acc1 = HaptAndTimestamp((data1.clone(), time1));

    //12ms経過時のデータ
    let data2 = vec!(2u8, 10u8);
    let time2 = Utc.timestamp(0, 12 * 1000 * 1000);
    let acc2 = HaptAndTimestamp((data2.clone(), time2));

    //18ms経過時のデータ
    let data3 = vec!(3u8, 10u8);
    let time3 = Utc.timestamp(0, 18 * 1000 * 1000);
    let acc3 = HaptAndTimestamp((data3.clone(), time3));

    //19ms経過時のデータ
    let data4 = vec!(4u8, 10u8);
    let time4 = Utc.timestamp(0, 19 * 1000 * 1000);
    let acc4 = HaptAndTimestamp((data4.clone(), time4));


    //2ms経過時のRTPが再生開始される
    //60fpsのため2ms以上18.6666...ms未満までは再生して良い
    //但し端数が面倒なので1ms手前から16ms後まで再生することにする
    let time_ts = Utc.timestamp(0, 2 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let data = extract_playabledata(vec!(acc0.clone(), acc1.clone(), acc2.clone(), acc3.clone(), acc4.clone()), &PlayTimingGap((time_ts, time_pt)));
    assert_eq!(data.0, vec!(acc4));
    assert_eq!(data.1, PlayDataAndTime((vec!(data1, data2, data3), time_pt)));
}

#[test]
fn test_check_time_insert_data() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(10, 0);
    let acc0 = HaptAndTimestamp((data0, time0));

    let data1 = vec!(0u8, 10u8);
    let time1 = Utc.timestamp(10, 100);
    let acc1 = HaptAndTimestamp((data1, time1));

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);
    let sum = check_time((sender, vec!(), pt.clone()), (Some(acc0.clone()), None));
    let sum = check_time(sum, (Some(acc1.clone()), None));
    assert_eq!(sum.1, vec!(acc0, acc1));
    assert_eq!(sum.2, pt);
}

#[test]
fn test_check_time_extract_data() {
    //0ms経過時のデータ
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(10, 0);
    let acc0 = HaptAndTimestamp((data0.clone(), time0));

    //1ms経過時のデータ
    let data1 = vec!(1u8, 10u8);
    let time1 = Utc.timestamp(10, 1 * 1000 * 1000);
    let acc1 = HaptAndTimestamp((data1.clone(), time1));

    //12ms経過時のデータ
    let data2 = vec!(2u8, 10u8);
    let time2 = Utc.timestamp(10, 12 * 1000 * 1000);
    let acc2 = HaptAndTimestamp((data2.clone(), time2));

    //18ms経過時のデータ
    let data3 = vec!(3u8, 10u8);
    let time3 = Utc.timestamp(10, 18 * 1000 * 1000);
    let acc3 = HaptAndTimestamp((data3.clone(), time3));

    //19ms経過時のデータ
    let data4 = vec!(4u8, 10u8);
    let time4 = Utc.timestamp(10, 19 * 1000 * 1000);
    let acc4 = HaptAndTimestamp((data4, time4));


    //2ms経過時のRTPが再生開始される
    //60fpsのため2ms以上18.6666...ms未満までは再生して良い
    //但し端数が面倒なので1ms手前から16.666ms後まで再生することにする
    let time_ts = Utc.timestamp(10, 2 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let sum = check_time((sender, vec!(), pt), (Some(acc0.clone()), None));
    let sum = check_time(sum, (Some(acc1.clone()), None));
    let sum = check_time(sum, (Some(acc2.clone()), None));
    let sum = check_time(sum, (Some(acc3.clone()), None));
    let sum = check_time(sum, (Some(acc4.clone()), None));
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts, time_pt)))));
    let x = receiver.wait().next().unwrap();
    assert_eq!(x, Ok(PlayDataAndTime((
        vec!(data1, data2, data3),
        time_pt))));
    assert_eq!(sum.1, vec!(acc4));
}

#[test]
fn test_check_time_insert_time() {
    let time_ts = Utc.timestamp(10, 2 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));
    let sum = check_time((sender, vec!(), pt), (None, Some(PlayTimingGap((time_ts, time_pt)))));
    assert_eq!(sum.1, vec!());
    assert_eq!(sum.2, PlayTimingGap((time_ts, time_pt)));
}

#[test]
fn test_check_time_insert_time_many_times() {
    let time_ts = Utc.timestamp(10, 2 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let time_ts2 = Utc.timestamp(10, 12 * 1000 * 1000);
    let time_ts3 = Utc.timestamp(10, 22 * 1000 * 1000);
    let time_ts4 = Utc.timestamp(10, 32 * 1000 * 1000);
    let time_ts5 = Utc.timestamp(10, 42 * 1000 * 1000);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);
    let sum = check_time((sender, vec!(), pt), (None, Some(PlayTimingGap((time_ts, time_pt)))));
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts2, time_pt)))));
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts3, time_pt)))));
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts4, time_pt)))));
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts5, time_pt)))));
    assert_eq!(sum.1, vec!());
    assert_eq!(sum.2, PlayTimingGap((time_ts5, time_pt)));
}

#[test]
fn test_check_time_insert_time_and_data_too_old() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HaptAndTimestamp((data0.clone(), time0));

    let time_ts = Utc.timestamp(10, 20 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);
    let sum = check_time((sender, vec!(), pt), (None, Some(PlayTimingGap((time_ts, time_pt)))));
    let sum = check_time(sum, (Some(acc0.clone()), None));
    assert_eq!(sum.1, vec!());
    assert_eq!(sum.2, PlayTimingGap((time_ts, time_pt)));
}

#[test]
fn test_check_time_insert_time_and_data_playable() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(10, 22 * 1000 * 1000);
    let acc0 = HaptAndTimestamp((data0.clone(), time0));

    let time_ts = Utc.timestamp(10, 20 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);


    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);
    let time_pt2 = Utc.timestamp(10, 0);
    let sum = check_time((sender, vec!(), pt), (None, Some(PlayTimingGap((time_ts, time_pt)))));
    let sum = check_time(sum, (Some(acc0), None));

    let x = receiver.wait().next().unwrap();
    assert_eq!(x, Ok(PlayDataAndTime((vec!(data0), time_pt2))));
    assert_eq!(sum.1, vec!());
    assert_eq!(sum.2, PlayTimingGap((time_ts, time_pt)));
}

#[test]
fn test_check_time_insert_time_and_data_nonplayable() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(10, 122 * 1000 * 1000);
    let acc0 = HaptAndTimestamp((data0.clone(), time0));

    let time_ts = Utc.timestamp(10, 20 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);
    let time_pt2 = Utc.timestamp(10, 0);
    let sum = check_time((sender, vec!(), pt), (None, Some(PlayTimingGap((time_ts, time_pt)))));
    let sum = check_time(sum, (Some(acc0.clone()), None));

    assert_eq!(sum.1, vec!(acc0));
    assert_eq!(sum.2, PlayTimingGap((time_ts, time_pt)));
}


