#![feature(drain_filter)]

extern crate futures;
extern crate tokio_core;
extern crate chrono;
use chrono::prelude::*;
use chrono::Duration;
use futures::*;
use futures::sync::mpsc;

fn main() {
    println!("Hello, world!");
}

#[derive(PartialEq, PartialOrd, Clone, Debug)]
struct HaptAndTimestamp(pub (Vec<u8>, DateTime<Utc>)); //data from haptic receiver
#[derive(PartialEq, PartialOrd, Clone, Debug)]
struct PlayTimingGap(pub (DateTime<Utc>, DateTime<Utc>)); //data from gstreamer

fn extract_playabledata(mut data: Vec<HaptAndTimestamp>, PlayTimingGap(time): PlayTimingGap) -> (Vec<HaptAndTimestamp>, Vec<HaptAndTimestamp>) {
    let playable_data = data.drain_filter(|&mut HaptAndTimestamp(ref x)| {
        x.1 < time.0 + Duration::milliseconds(13)
    }).map(|HaptAndTimestamp(x)| HaptAndTimestamp((x.0, time.1))).collect::<Vec<_>>();

    (playable_data, data)
}

fn check_time(sum: (mpsc::Sender<Vec<HaptAndTimestamp>>, Vec<HaptAndTimestamp>, Vec<PlayTimingGap>), acc: (Option<HaptAndTimestamp>, Option<PlayTimingGap>)) -> (mpsc::Sender<Vec<HaptAndTimestamp>>, Vec<HaptAndTimestamp>, Vec<PlayTimingGap>){
    match acc {
        (Some(t), _) => {
            let mut s = sum.1;
            s.push(t);
            (sum.0, s, sum.2)
        },
        (_, Some(p)) => {
            let (playable, nonplayable) = extract_playabledata(sum.1, p);
            let sender = sum.0.send(playable).wait().unwrap();
            (sender, nonplayable, sum.2)
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
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HaptAndTimestamp((data0.clone(), time0));

    let data1 = vec!(0u8, 10u8);
    let time1 = Utc.timestamp(0, 12 * 1000 * 1000);
    let acc1 = HaptAndTimestamp((data1.clone(), time1));

    let data2 = vec!(0u8, 10u8);
    let time2 = Utc.timestamp(0, 14 * 1000 * 1000);
    let acc2 = HaptAndTimestamp((data2, time2));

    let time_ts = Utc.timestamp(0, 1);
    let time_pt = Utc.timestamp(10, 0);

    let data = extract_playabledata(vec!(acc0.clone(), acc1.clone(), acc2.clone()), PlayTimingGap((time_ts, time_pt)));
    assert_eq!(data.0, vec!(HaptAndTimestamp((data0, time_pt)), HaptAndTimestamp((data1, time_pt))));
    assert_eq!(data.1, vec!(acc2));
}

#[test]
fn test_check_time_insert_data() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HaptAndTimestamp((data0, time0));

    let data1 = vec!(0u8, 10u8);
    let time1 = Utc.timestamp(0, 100);
    let acc1 = HaptAndTimestamp((data1, time1));

    let (sender, receiver) = mpsc::channel::<Vec<HaptAndTimestamp>>(5000);
    let sum = check_time((sender, vec!(), vec!()), (Some(acc0.clone()), None));
    let sum = check_time(sum, (Some(acc1.clone()), None));
    assert_eq!(sum.1, vec!(acc0, acc1));
    assert_eq!(sum.2, vec!());
}

#[test]
fn test_check_time_extract_data() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HaptAndTimestamp((data0.clone(), time0));

    let data1 = vec!(0u8, 10u8);
    let time1 = Utc.timestamp(0, 12 * 1000 * 1000);
    let acc1 = HaptAndTimestamp((data1.clone(), time1));

    let data2 = vec!(0u8, 10u8);
    let time2 = Utc.timestamp(0, 14 * 1000 * 1000);
    let acc2 = HaptAndTimestamp((data2, time2));

    let time_ts = Utc.timestamp(0, 1);
    let time_pt = Utc.timestamp(10, 0);

    let (sender, receiver) = mpsc::channel::<Vec<HaptAndTimestamp>>(5000);

    let sum = check_time((sender, vec!(), vec!()), (Some(acc0.clone()), None));
    let sum = check_time(sum, (Some(acc1.clone()), None));
    let sum = check_time(sum, (Some(acc2.clone()), None));
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts, time_pt)))));
    let x = receiver.wait().next().unwrap();
    assert_eq!(x, Ok(vec!(
        HaptAndTimestamp((data0, time_pt)),
        HaptAndTimestamp((data1, time_pt))
    )));
    assert_eq!(sum.1, vec!(acc2));
}
