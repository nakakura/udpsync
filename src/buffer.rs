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
    let offset_ms = (*OFFSET.read().unwrap()) as i64;
    let item_time = item.timestamp + Duration::milliseconds(offset_ms);

    if time_vec.len() == 0 {
        //1つもtimestampがなければためとく
        data.push(item);
        return (data, time_vec, None);
    }

    let first = time_vec[0].clone();
    if item.timestamp < first.timestamp() - Duration::milliseconds(16) {
        //最初のtimestampより小さくて、極端に小さい場合は捨てる
        return (data, time_vec, None);
    } else if item.timestamp < first.timestamp() {
        //最初のtimestampより小さくて、極端に小さくない場合は
        //最初のtimestampから計算した時間に送信
        let message = format!("case 1");
        let playtime = first.play_time() - (first.timestamp().signed_duration_since(item.timestamp));
        return (data, time_vec, Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))));
    }

    //該当データよりも大きなtimestampを探す
    let index_opt = {
        let found_opt = time_vec.iter().enumerate().find(|&(ref i, ref data)| {
            item_time < data.timestamp()
        });

        if let Some(found) = found_opt {
            Some((found.0, found.1.clone()))
        } else {
            None
        }
    };
    if let Some(found_item) = index_opt {
        //該当データよりも大きなtimestampがあった場合
        //前後のデータから計算した時刻に送信
        if found_item.0 > 0 {
            let before = time_vec[found_item.0 - 1].clone();
            let next = time_vec[found_item.0].clone();
            let (index, closest_item) = if (item.timestamp.signed_duration_since(before.timestamp()) < next.timestamp().signed_duration_since(item.timestamp)) {
                (found_item.0-1, before)
            } else {
                (found_item.0, next)
            };

            let message = format!("case 2 index {}", index);
            eprintln!("case 2 {:?} {:?}", closest_item.timestamp(), item.timestamp);
            eprintln!("case 2 {:?} ", closest_item.play_time());
            eprintln!("diff {:?}", closest_item.timestamp().signed_duration_since(item.timestamp));
            let playtime = closest_item.play_time() - closest_item.timestamp().signed_duration_since(item.timestamp);
            if found_item.0 > 50 {
                //time_vecが長すぎる場合は減らす
                return (data, time_vec.split_off(25), Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))));
            } else {
                return (data, time_vec, Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))));
            }
        } else {
            let message = format!("case 3 index {}", found_item.0);
            let playtime = found_item.1.play_time() - found_item.1.timestamp().signed_duration_since(item.timestamp);
            if found_item.0 > 50 {
                //time_vecが長すぎる場合は減らす
                return (data, time_vec.split_off(25), Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))));
            } else {
                return (data, time_vec, Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))));
            }
        }
    } else {
        let last_item = time_vec[time_vec.len()-1].clone();
        if item.timestamp < last_item.timestamp() + Duration::milliseconds(16 * 3) {
            eprintln!("hoge");
            //該当データよりも大きなtimestampがなかったけど、
            //最後のやつからそんなに離れてない場合はそこから計算して送る
            let message = format!("case 4 index {}", time_vec.len() - 1);
            let playtime = last_item.play_time() - last_item.timestamp().signed_duration_since(item.timestamp);
            return (data, time_vec, Some(PlayDataAndTime((vec!(message.into_bytes()), playtime))));
        }
    }

    eprintln!("moge");
    //ここに来る場合、新しすぎるデータ
    //とりあえずためておく
    data.push(item);
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
    let message = format!("case 2 index {}", 0);
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
    let message = format!("case 4 index {}", 1);
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