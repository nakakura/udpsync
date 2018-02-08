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

impl PlayDataAndTime {
    pub fn len(&self) -> usize {
        let &PlayDataAndTime((ref vec, ref _time)) = self;
        vec.len()
    }
}

pub fn set_offset(offset: i32) {
    *OFFSET.write().unwrap() = offset;
}

fn extract_playabledata(mut data: Vec<HapticData>, &PlayTimingGap(time): &PlayTimingGap) -> (Vec<HapticData>, PlayDataAndTime) {
    let offset = (*OFFSET.read().unwrap()) as i64;
    //古すぎるデータは再生せず捨てる
    let _too_old_data = data.drain_filter(|ref mut x| {
        x.timestamp < time.0 - Duration::milliseconds(16) + Duration::milliseconds(offset)
    }).collect::<Vec<_>>();

    if _too_old_data.len() > 0 {
        println!("too old {:?}\n{:?}", _too_old_data, time.0);
    }

    //再生データの取り出し
    let playable_data = data.drain_filter(|ref mut x| {
        x.timestamp <= time.0 + Duration::microseconds(16666) + Duration::milliseconds(offset)
    }).map(|x| x.buf).collect::<Vec<_>>();

    (data, PlayDataAndTime((playable_data, time.1)))
}

/// 触覚センサから来たタイムスタンプ付きデータと、
/// gstreamerからやってきたRTPタイムスタンプと再生時刻のペアを突き合わせ、再生可能なものをfutureとして吐き出す
/// 但し、触覚センサは送信側ローカルクロック、
/// RTPタイムスタンプと再生時刻のペアはそれぞれ再生開始時点からの相対位置であるので、
/// この関数に入れる前に受信側ローカルクロックへ変換が必要である
/// 送信するときは再生予定時刻を付けて送る
pub fn check_time(sum: (mpsc::Sender<PlayDataAndTime>, Vec<HapticData>, Option<PlayTimingGap>), acc: (Option<HapticData>, Option<PlayTimingGap>)) -> Result<(mpsc::Sender<PlayDataAndTime>, Vec<HapticData>, Option<PlayTimingGap>), ()>{
    match acc {
        (Some(t), _) => {
            let mut data = sum.1;
            data.push(t);
            if let Some(ref gap) = sum.2 {
                let (nonplayable, play_data) = extract_playabledata(data, gap);
                let sender = if play_data.len() > 0 {
                    sum.0.send(play_data).wait().unwrap()
                } else {
                    sum.0
                };

                Ok((sender, nonplayable, Some(gap.clone())))
            } else {
                Ok((sum.0 ,data, None))
            }
       },
        (_, Some(p)) => {
            let (nonplayable, play_data) = extract_playabledata(sum.1, &p);
            let sender = if play_data.len() > 0 {
                sum.0.send(play_data).wait().unwrap()
            } else {
                sum.0
            };

            Ok((sender, nonplayable, Some(p)))
        },
        _ => {
            Ok(sum)
        }
    }
}

#[test]
fn test_extract_playable_data() {
    //0ms経過時のデータ
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HapticData::new(data0.clone(), time0);

    //1ms経過時のデータ
    let data1 = vec!(1u8, 10u8);
    let time1 = Utc.timestamp(0, 1 * 1000 * 1000);
    let acc1 = HapticData::new(data1.clone(), time1);

    //12ms経過時のデータ
    let data2 = vec!(2u8, 10u8);
    let time2 = Utc.timestamp(0, 12 * 1000 * 1000);
    let acc2 = HapticData::new(data2.clone(), time2);

    //18ms経過時のデータ
    let data3 = vec!(3u8, 10u8);
    let time3 = Utc.timestamp(0, 18 * 1000 * 1000);
    let acc3 = HapticData::new(data3.clone(), time3);

    //19ms経過時のデータ
    let data4 = vec!(4u8, 10u8);
    let time4 = Utc.timestamp(0, 19 * 1000 * 1000);
    let acc4 = HapticData::new(data4.clone(), time4);

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
    let acc0 = HapticData::new(data0.clone(), time0);

    let data1 = vec!(0u8, 10u8);
    let time1 = Utc.timestamp(10, 100);
    let acc1 = HapticData::new(data1.clone(), time1);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, _) = mpsc::channel::<PlayDataAndTime>(5000);
    let sum = check_time((sender, vec!(), Some(pt.clone())), (Some(acc0.clone()), None)).unwrap();
    let sum = check_time(sum, (Some(acc1.clone()), None)).unwrap();
    assert_eq!(sum.1, vec!(acc0, acc1));
    assert_eq!(sum.2, Some(pt));
}

#[test]
fn test_check_time_extract_data() {
    //0ms経過時のデータ
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(10, 0);
    let acc0 = HapticData::new(data0.clone(), time0);

    //1ms経過時のデータ
    let data1 = vec!(1u8, 10u8);
    let time1 = Utc.timestamp(10, 1 * 1000 * 1000);
    let acc1 = HapticData::new(data1.clone(), time1);

    //12ms経過時のデータ
    let data2 = vec!(2u8, 10u8);
    let time2 = Utc.timestamp(10, 12 * 1000 * 1000);
    let acc2 = HapticData::new(data2.clone(), time2);

    //18ms経過時のデータ
    let data3 = vec!(3u8, 10u8);
    let time3 = Utc.timestamp(10, 18 * 1000 * 1000);
    let acc3 = HapticData::new(data3.clone(), time3);

    //19ms経過時のデータ
    let data4 = vec!(4u8, 10u8);
    let time4 = Utc.timestamp(10, 19 * 1000 * 1000);
    let acc4 = HapticData::new(data4.clone(), time4);


    //2ms経過時のRTPが再生開始される
    //60fpsのため2ms以上18.6666...ms未満までは再生して良い
    //但し端数が面倒なので1ms手前から16.666ms後まで再生することにする
    let time_ts = Utc.timestamp(10, 2 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let sum = check_time((sender, vec!(), Some(pt)), (Some(acc0.clone()), None)).unwrap();
    let sum = check_time(sum, (Some(acc1.clone()), None)).unwrap();
    let sum = check_time(sum, (Some(acc2.clone()), None)).unwrap();
    let sum = check_time(sum, (Some(acc3.clone()), None)).unwrap();
    let sum = check_time(sum, (Some(acc4.clone()), None)).unwrap();
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts, time_pt))))).unwrap();
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

    let (sender, _) = mpsc::channel::<PlayDataAndTime>(5000);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));
    let sum = check_time((sender, vec!(), Some(pt)), (None, Some(PlayTimingGap((time_ts, time_pt))))).unwrap();
    assert_eq!(sum.1, vec!());
    assert_eq!(sum.2, Some(PlayTimingGap((time_ts, time_pt))));
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

    let (sender, _) = mpsc::channel::<PlayDataAndTime>(5000);
    let sum = check_time((sender, vec!(), Some(pt)), (None, Some(PlayTimingGap((time_ts, time_pt))))).unwrap();
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts2, time_pt))))).unwrap();
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts3, time_pt))))).unwrap();
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts4, time_pt))))).unwrap();
    let sum = check_time(sum, (None, Some(PlayTimingGap((time_ts5, time_pt))))).unwrap();
    assert_eq!(sum.1, vec!());
    assert_eq!(sum.2, Some(PlayTimingGap((time_ts5, time_pt))));
}

#[test]
fn test_check_time_insert_time_and_data_too_old() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 0);
    let acc0 = HapticData::new(data0.clone(), time0);

    let time_ts = Utc.timestamp(10, 20 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, _) = mpsc::channel::<PlayDataAndTime>(5000);
    let sum = check_time((sender, vec!(), Some(pt)), (None, Some(PlayTimingGap((time_ts, time_pt))))).unwrap();
    let sum = check_time(sum, (Some(acc0.clone()), None)).unwrap();
    assert_eq!(sum.1, vec!());
    assert_eq!(sum.2.unwrap(), PlayTimingGap((time_ts, time_pt)));
}

#[test]
fn test_check_time_insert_time_and_data_playable() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(10, 22 * 1000 * 1000);
    let acc0 = HapticData::new(data0.clone(), time0);

    let time_ts = Utc.timestamp(10, 20 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);


    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, receiver) = mpsc::channel::<PlayDataAndTime>(5000);
    let time_pt2 = Utc.timestamp(10, 0);
    let sum = check_time((sender, vec!(), Some(pt)), (None, Some(PlayTimingGap((time_ts, time_pt))))).unwrap();
    let sum = check_time(sum, (Some(acc0), None)).unwrap();

    let x = receiver.wait().next().unwrap();
    assert_eq!(x, Ok(PlayDataAndTime((vec!(data0), time_pt2))));
    assert_eq!(sum.1, vec!());
    assert_eq!(sum.2.unwrap(), PlayTimingGap((time_ts, time_pt)));
}

#[test]
fn test_check_time_insert_time_and_data_nonplayable() {
    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(10, 122 * 1000 * 1000);
    let acc0 = HapticData::new(data0.clone(), time0);

    let time_ts = Utc.timestamp(10, 20 * 1000 * 1000);
    let time_pt = Utc.timestamp(10, 0);

    let time_origin = Utc.timestamp(0, 0);
    let pt = PlayTimingGap((time_origin, time_origin));

    let (sender, _) = mpsc::channel::<PlayDataAndTime>(5000);
    let sum = check_time((sender, vec!(), Some(pt)), (None, Some(PlayTimingGap((time_ts, time_pt))))).unwrap();
    let sum = check_time(sum, (Some(acc0.clone()), None)).unwrap();

    assert_eq!(sum.1, vec!(acc0));
    assert_eq!(sum.2.unwrap(), PlayTimingGap((time_ts, time_pt)));
}
