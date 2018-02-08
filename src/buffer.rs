use chrono::prelude::*;
use chrono::Duration;
use futures::*;
use futures::Sink;
use futures::sync::mpsc;

use std::sync::RwLock;

use haptic_data::HapticData;

lazy_static! {
    pub static ref OFFSET: RwLock<i32> = {
        RwLock::new(0)
    };
}

#[derive(PartialEq, PartialOrd, Clone, Debug)]
pub struct PlayTimingGap(pub (DateTime<Utc>, DateTime<Utc>)); //data from gstreamer
#[derive(PartialEq, PartialOrd, Clone, Debug)]
pub struct PlayDataAndTime(pub (Vec<Vec<u8>>, DateTime<Utc>)); //data for send

impl PlayTimingGap {
    pub fn timestamp(&self) -> DateTime<Utc> {
        let &PlayTimingGap(ref inner) = self;
        inner.0
    }

    pub fn play_time(&self) -> DateTime<Utc> {
        let &PlayTimingGap(ref inner) = self;
        inner.1
    }
}

impl PlayDataAndTime {
    pub fn len(&self) -> usize {
        let &PlayDataAndTime((ref vec, ref _time)) = self;
        vec.len()
    }
}

pub fn set_offset(offset: i32) {
    *OFFSET.write().unwrap() = offset;
}

fn insert_data(mut data: Vec<HapticData>, mut time_vec: Vec<PlayTimingGap>, item: HapticData) -> (Vec<HapticData>, Vec<PlayTimingGap>, Option<PlayDataAndTime>) {
    //timeが無い場合はとりあえずdataを残しとく
    if time_vec.len() == 0 {
        data.push(item);
        return (data, time_vec, None);
    }

    let offset_ms = (*OFFSET.read().unwrap()) as i64;
    let item_time = item.timestamp + Duration::milliseconds(offset_ms);

    //古すぎるdataだと残す意味ないので捨てる
    if item_time + Duration::milliseconds(17) < time_vec[0].timestamp() {
        return (data, time_vec, None);
    }

    //古すぎるデータは再生せず捨てる
    let _too_old_time = time_vec.drain_filter(|ref mut x| {
        item_time > x.timestamp()
    }).collect::<Vec<_>>();

    if time_vec.len() > 0 {
        let play_time = time_vec[0].play_time();
        (data, time_vec, Some(PlayDataAndTime((vec!(item.buf), play_time))))
    } else {
        data.push(item);
        (data, time_vec, None)
    }
}

fn insert_time(mut data: Vec<HapticData>, mut time_vec: Vec<PlayTimingGap>, PlayTimingGap(time): PlayTimingGap) -> (Vec<HapticData>, Vec<PlayTimingGap>, Option<PlayDataAndTime>) {
    //dataが無い場合はとりあえずtimeを残しとく
    if data.len() == 0 {
        time_vec.push(PlayTimingGap(time));
        return (data, time_vec, None);
    }

    let offset_ms = (*OFFSET.read().unwrap()) as i64;
    let base_time = time.0 - Duration::milliseconds(offset_ms);

    //古すぎるTimeだと残す意味ないので捨てる
    if data[0].timestamp > base_time {
        return (data, time_vec, None);
    }

    //古すぎるデータは再生せず捨てる
    let _too_old_data = data.drain_filter(|ref mut x| {
        x.timestamp < base_time - Duration::milliseconds(17)
    }).collect::<Vec<_>>();

    //再生データの取り出し
    let playable_data = data.drain_filter(|ref mut x| {
        x.timestamp <= base_time
    }).map(|x| x.buf).collect::<Vec<_>>();

    time_vec.push(PlayTimingGap(time));

    if playable_data.len() > 0 {
        (data, time_vec, Some(PlayDataAndTime((playable_data, time.1))))
    } else {
        (data, time_vec, None)
    }
}

/// 触覚センサから来たタイムスタンプ付きデータと、
/// gstreamerからやってきたRTPタイムスタンプと再生時刻のペアを突き合わせ、再生可能なものをfutureとして吐き出す
/// 但し、触覚センサは送信側ローカルクロック、
/// RTPタイムスタンプと再生時刻のペアはそれぞれ再生開始時点からの相対位置であるので、
/// この関数に入れる前に受信側ローカルクロックへ変換が必要である
/// 送信するときは再生予定時刻を付けて送る
pub fn check_time(sum: (mpsc::Sender<PlayDataAndTime>, Vec<HapticData>, Vec<PlayTimingGap>), acc: (Option<HapticData>, Option<PlayTimingGap>)) -> Result<(mpsc::Sender<PlayDataAndTime>, Vec<HapticData>, Vec<PlayTimingGap>), ()>{
    match acc {
        (Some(t), _) => {
            let (data, time, playable) = insert_data(sum.1, sum.2, t);
            if let Some(send_data) = playable {
                let sender = sum.0.send(send_data).wait().unwrap();
                Ok((sender, data, time))
            } else {
                Ok((sum.0, data, time))
            }
       },
        (_, Some(p)) => {
            let (data, time, playable) = insert_time(sum.1, sum.2, p);
            if let Some(send_data) = playable {
                let sender = sum.0.send(send_data).wait().unwrap();
                Ok((sender, data, time))
            } else {
                Ok((sum.0, data, time))
            }

        },
        _ => {
            Ok(sum)
        }
    }
}

#[test]
fn test_insert_data_first_data() {
    //0ms経過時のデータ
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HapticData::new(data0.clone(), time0);
    let (data, time, playable) = insert_data(vec!(), vec!(), acc0.clone());
    assert_eq!(data, vec!(acc0.clone()));
    assert_eq!(time, vec!());
    assert_eq!(playable, None);

    let data1 = vec!(1u8, 10u8);
    let time1 = Utc.timestamp(1, 1 * 1000 * 1000);
    let acc1 = HapticData::new(data1.clone(), time1);
    let (data, time, playable) = insert_data(vec!(acc0.clone()), vec!(), acc1.clone());
    assert_eq!(data, vec!(acc0, acc1));
    assert_eq!(time, vec!());
    assert_eq!(playable, None);
}

#[test]
fn test_insert_data_too_old() {
    let timestamp0 = Utc.timestamp(50, 0);
    let play_time0 = Utc.timestamp(0, 0);
    let timestamp1 = Utc.timestamp(66, 1 * 1000 * 1000);
    let play_time1 = Utc.timestamp(66, 1 * 1000 * 1000);

    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HapticData::new(data0.clone(), time0);

    let (data, time, playable) = insert_data(vec!(), vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))), acc0);
    assert_eq!(data, vec!());
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))));
    assert_eq!(playable, None);
}

#[test]
fn test_insert_data_playable() {
    let timestamp0 = Utc.timestamp(0, 50 * 1000 * 1000);
    let play_time0 = Utc.timestamp(0, 1 * 1000 * 1000);
    let timestamp1 = Utc.timestamp(0, 66 * 1000 * 1000);
    let play_time1 = Utc.timestamp(0, 1 * 1000 * 1000);

    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 40 * 1000 * 1000);
    let acc0 = HapticData::new(data0.clone(), time0);

    let (data, time, playable) = insert_data(vec!(), vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))), acc0.clone());
    assert_eq!(data, vec!());
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))));
    let playable_data = PlayDataAndTime((vec!(acc0.buf), play_time0));
    assert_eq!(playable, Some(playable_data));
}

#[test]
fn test_insert_data_too_new() {
    let timestamp0 = Utc.timestamp(0, 50 * 1000 * 1000);
    let play_time0 = Utc.timestamp(0, 1 * 1000 * 1000);
    let timestamp1 = Utc.timestamp(0, 66 * 1000 * 1000);
    let play_time1 = Utc.timestamp(0, 1 * 1000 * 1000);

    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(10, 0);
    let acc0 = HapticData::new(data0.clone(), time0);

    let (data, time, playable) = insert_data(vec!(), vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))), acc0.clone());
    assert_eq!(data, vec!(acc0));
    assert_eq!(time, vec!());
    assert_eq!(playable, None);
}

#[test]
fn test_insert_time_first_time() {
    let timestamp0 = Utc.timestamp(0, 0);
    let play_time0 = Utc.timestamp(0, 0);

    let (data, time, playable) = insert_time(vec!(), vec!(), PlayTimingGap((timestamp0, play_time0)));
    assert_eq!(data, vec!());
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0))));
    assert_eq!(playable, None);

    let timestamp1 = Utc.timestamp(1, 1 * 1000 * 1000);
    let play_time1 = Utc.timestamp(1, 1 * 1000 * 1000);

    let (data, time, playable) = insert_time(vec!(), vec!(PlayTimingGap((timestamp0, play_time0))), PlayTimingGap((timestamp1, play_time1)));
    assert_eq!(data, vec!());
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))));
    assert_eq!(playable, None);
}

#[test]
fn test_insert_time_too_old(){
    //0ms経過時のデータ
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(1, 0);
    let acc0 = HapticData::new(data0.clone(), time0);

    let data1 = vec!(1u8, 10u8);
    let time1 = Utc.timestamp(1, 1 * 1000 * 1000);
    let acc1 = HapticData::new(data1.clone(), time1);

    let timestamp0 = Utc.timestamp(0, 0);
    let play_time0 = Utc.timestamp(0, 0);

    let (data, time, playable) = insert_time(vec!(acc0.clone(), acc1.clone()), vec!(), PlayTimingGap((timestamp0, play_time0)));

    assert_eq!(data, vec!(acc0, acc1));
    assert_eq!(time, vec!());
    assert_eq!(playable, None);
}

#[test]
fn test_insert_time_playable(){
    //0ms経過時のデータ
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HapticData::new(data0.clone(), time0);

    let data1 = vec!(1u8, 10u8);
    let time1 = Utc.timestamp(0, 10 * 1000 * 1000);
    let acc1 = HapticData::new(data1.clone(), time1);

    let timestamp0 = Utc.timestamp(0, 15 * 1000 * 1000);
    let play_time0 = Utc.timestamp(0, 10 * 1000 * 1000);

    let (data, time, playable) = insert_time(vec!(acc0.clone(), acc1.clone()), vec!(), PlayTimingGap((timestamp0, play_time0)));

    assert_eq!(data, vec!());
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0))));
    assert_eq!(playable, Some(PlayDataAndTime((vec!(acc0.buf, acc1.buf), play_time0))));
}

#[test]
fn test_insert_time_too_new(){
    //0ms経過時のデータ
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HapticData::new(data0.clone(), time0);

    let data1 = vec!(1u8, 10u8);
    let time1 = Utc.timestamp(0, 10 * 1000 * 1000);
    let acc1 = HapticData::new(data1.clone(), time1);

    let timestamp0 = Utc.timestamp(0, 45 * 1000 * 1000);
    let play_time0 = Utc.timestamp(0, 10 * 1000 * 1000);

    let (data, time, playable) = insert_time(vec!(acc0.clone(), acc1.clone()), vec!(), PlayTimingGap((timestamp0, play_time0)));

    assert_eq!(data, vec!());
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0))));
    assert_eq!(playable, None);
}