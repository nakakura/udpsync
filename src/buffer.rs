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

fn find(time_vec: &Vec<PlayTimingGap>, time: DateTime<Utc>) -> (usize, Option<usize>) {
    if time_vec.len() == 0 {
        return (0, None);
    }

    if time > time_vec[time_vec.len()-1].timestamp() {
        return (2, None);
    }

    let found_opt = time_vec.iter().enumerate().find(|&(ref i, ref data)| {
        time < data.timestamp()
    });

    (1, found_opt.map(|(i, j)| i))
}

fn insert_data(mut data: Vec<HapticData>, mut time_vec: Vec<PlayTimingGap>, item: HapticData) -> (Vec<HapticData>, Vec<PlayTimingGap>, Option<PlayDataAndTime>) {
    let offset_ms = (*OFFSET.read().unwrap()) as i64;
    let item_time = item.timestamp + Duration::milliseconds(offset_ms);
    let (flag, index_opt) = find(&time_vec, item.timestamp);
    if flag == 0 {
        data.push(item);
        return (data, time_vec, None);
    } else if flag == 2 {
        let comp = time_vec[time_vec.len() - 1].clone();
        if item.timestamp.signed_duration_since(comp.timestamp()) < Duration::milliseconds(32) {
            let playtime = comp.play_time() + item.timestamp.signed_duration_since(comp.timestamp());
            let message = format!("bigger than all data");
            return (data, time_vec, Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))))
        } else {
            data.push(item);
            return (data, time_vec, None);
        }
    } else {
        if let Some(index) = index_opt {
            if index == 0 {
                let comp = time_vec[index].clone();
                let playtime = comp.play_time() + item.timestamp.signed_duration_since(comp.timestamp());
                let message = format!("smaller than all data");
                return (data, time_vec, Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))))
            } else {
                let comp1 = time_vec[index - 1].clone();
                let comp2 = time_vec[index].clone();
                if item.timestamp.signed_duration_since(comp1.timestamp()) < comp2.timestamp().signed_duration_since(item.timestamp) {
                    let playtime = comp1.play_time() + item.timestamp.signed_duration_since(comp1.timestamp());
                    let message = format!("adjust from smaller one {}", index-1);
                    return (data, time_vec, Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))))
                } else {
                    let playtime = comp2.play_time() + item.timestamp.signed_duration_since(comp2.timestamp());
                    let message = format!("adjust from bigger one {}", index);
                    return (data, time_vec, Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))))
                }
            }
        }
    }

    return (data, time_vec, None);
}

fn insert_time(mut data: Vec<HapticData>, mut time_vec: Vec<PlayTimingGap>, time: PlayTimingGap) -> (Vec<HapticData>, Vec<PlayTimingGap>, Option<PlayDataAndTime>) {
    time_vec.push(time.clone());
    let offset_ms = (*OFFSET.read().unwrap()) as i64;
    let base_time = time.timestamp() - Duration::milliseconds(offset_ms);

    let send_items: Vec<Vec<u8>> = data.drain_filter(|ref mut x| {
        base_time - Duration::milliseconds(16) < x.timestamp && x.timestamp <= base_time
    }).map(|_i| {
        let message = format!("from insert_time");
        message.into_bytes()
    }).collect();

    if send_items.len() > 0 {
        (data, time_vec, Some(PlayDataAndTime((send_items, time.play_time()))))
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

/*
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
*/

#[test]
fn test_insert_data_playable() {
    let timestamp0 = Utc.timestamp(0, 50 * 1000 * 1000);
    let play_time0 = Utc.timestamp(0, 1 * 1000 * 1000);
    let timestamp1 = Utc.timestamp(0, 66 * 1000 * 1000);
    let play_time1 = Utc.timestamp(0, 10 * 1000 * 1000);

    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 55 * 1000 * 1000);
    let acc0 = HapticData::new(data0.clone(), time0);

    let (data, time, playable) = insert_data(vec!(), vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))), acc0.clone());
    assert_eq!(data, vec!());
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))));
    let message = format!("adjust from smaller one {}", 0);
    let playable_data = PlayDataAndTime((vec!(message.into_bytes()), play_time0 + Duration::milliseconds(55 - 50)));
    assert_eq!(playable, Some(playable_data));
}

#[test]
fn test_insert_a_little_bit_new_data_playable() {
    let timestamp0 = Utc.timestamp(0, 50 * 1000 * 1000);
    let play_time0 = Utc.timestamp(0, 1 * 1000 * 1000);
    let timestamp1 = Utc.timestamp(0, 66 * 1000 * 1000);
    let play_time1 = Utc.timestamp(0, 10 * 1000 * 1000);

    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(0, 70 * 1000 * 1000);
    let acc0 = HapticData::new(data0.clone(), time0);

    let (data, time, playable) = insert_data(vec!(), vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))), acc0.clone());
    assert_eq!(data, vec!());
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))));
    let message = format!("bigger than all data");
    let playable_data = PlayDataAndTime((vec!(message.into_bytes()), play_time1 + time0.signed_duration_since(timestamp1)));
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
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0)), PlayTimingGap((timestamp1, play_time1))));
    assert_eq!(playable, None);
}

/*
#[test]
fn test_insert_data_and_remove_too_old_time() {
    let timestamp0 = Utc.timestamp(1, 50 * 1000 * 1000);
    let play_time0 = Utc.timestamp(1, 1 * 1000 * 1000);
    let timestamp1 = Utc.timestamp(1, 66 * 1000 * 1000);
    let play_time1 = Utc.timestamp(1, 10 * 1000 * 1000);

    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(1, 55 * 1000 * 1000);
    let acc0 = HapticData::new(data0.clone(), time0);

    let timestamp_too_old = Utc.timestamp(0, 50 * 1000 * 1000);
    let play_time_too_old = Utc.timestamp(0, 1 * 1000 * 1000);
    let mut ts = vec!(PlayTimingGap((timestamp_too_old, play_time_too_old)));
    for i in 0..1000 {
        ts.push(PlayTimingGap((timestamp_too_old, play_time_too_old)));
    }
    ts.push(PlayTimingGap((timestamp0, play_time0)));
    ts.push(PlayTimingGap((timestamp1, play_time1)));

    let mut ts2 = vec!(PlayTimingGap((timestamp_too_old, play_time_too_old)));
    for i in 0..750 {
        ts2.push(PlayTimingGap((timestamp_too_old, play_time_too_old)));
    }
    ts2.push(PlayTimingGap((timestamp0, play_time0)));
    ts2.push(PlayTimingGap((timestamp1, play_time1)));

    let (data, time, playable) = insert_data(vec!(), ts.clone(), acc0.clone());
    assert_eq!(data, vec!());
    assert_eq!(time.len(), ts2.len());
    let playable_data = PlayDataAndTime((vec!(acc0.buf), play_time1));
    assert_eq!(playable, Some(playable_data));

    /*

    let timestamp0 = Utc.timestamp(1, 50 * 1000 * 1000);
    let play_time0 = Utc.timestamp(0, 1 * 1000 * 1000);
    let timestamp1 = Utc.timestamp(1, 66 * 1000 * 1000);
    let play_time1 = Utc.timestamp(0, 1 * 1000 * 1000);

    let data0 = vec!(0u8, 10u8);
    let time0 = Utc.timestamp(1, 51);
    let acc0 = HapticData::new(data0.clone(), time0);

   let (data, time, playable) = insert_data(vec!(), ts.clone(), acc0.clone());
    assert_eq!(data, vec!(acc0));
    assert_eq!(time.len(), ts.len());
    assert_eq!(playable, None);
    */
}
*/
/*
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

/*
#[test]
fn test_insert_time_too_old(){ //removeしない実装に一時的にした
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
*/


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

    assert_eq!(data, vec!(acc0, acc1));
    assert_eq!(time, vec!(PlayTimingGap((timestamp0, play_time0))));
    assert_eq!(playable, None);
}

#[test]
fn test_iter() {
    let foo = vec![1, 35, 64, 36, 26];
    foo.iter().enumerate().filter(|&(i, v)| {
        v % 2 == 0
    }).for_each(|i| {
        eprintln!("{:?}", i)
    });
}
*/