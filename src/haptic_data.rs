use chrono::*;
use bincode::{serialize, deserialize, Bounded, ErrorKind};

#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub struct HapticData {
    buf: Vec<u8>,
    timestamp: DateTime<Utc>,
}

impl HapticData {
    pub fn encode(&self) -> Result<Vec<u8>, Box<ErrorKind>> {
        let limit = Bounded(1500);
        let timestamp = self.timestamp.naive_utc();
        let sec = timestamp.timestamp();
        let nanosec = timestamp.timestamp_subsec_nanos();

        serialize(&(sec, nanosec, &self.buf), limit)
    }

    pub fn decode(data: &Vec<u8>) -> Self {
        let (sec, nanosec, buf): (i64, u32, Vec<u8>) = deserialize(data).unwrap();
        HapticData {
            buf: buf,
            timestamp: DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(sec, nanosec), Utc)
        }
    }
}

# [test]
fn test_timestamp() {
    let now = Utc::now();
    let timestamp = now.naive_utc();
    let sec = timestamp.timestamp();
    let nanosec = timestamp.timestamp_subsec_nanos();
    let now2 = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(sec, nanosec), Utc);
    assert_eq!(now, now2);
}

# [test]
fn test_enc_dec() {
    let data = HapticData {
        buf: vec!(1, 2, 3),
        timestamp: Utc::now()
    };
    let v = data.encode().unwrap();
    let data2 = HapticData::decode(&v);
    assert_eq!(data, data2);
}

