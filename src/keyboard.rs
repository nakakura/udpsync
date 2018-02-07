use std;
use std::str::FromStr;
use std::thread;
use std::sync::Arc;

pub fn get_keyboard<F>(func: F) -> thread::JoinHandle<()>
    where F: Send + 'static + Fn(i32) + Sync
{
    let func = Arc::new(func);
    let f = func.clone();
    thread::spawn(move || {
        loop {
            let scan = std::io::stdin();
            let mut line = String::new();
            let _ = scan.read_line(&mut line);
            let len = line.len();
            line.truncate(len - 1);
            let millsec = i32::from_str(line.as_str());
            let _ = millsec.map(|millsec| {
                f(millsec);
            });
        }
    })
}