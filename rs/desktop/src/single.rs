//! One instance at a time. The first instance holds a lock file in the data folder (the operating system drops it when the process ends,
//! even by a crash) and listens on a local port; a second start tells the first to show its window and exits.
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

pub struct Guard { _lock: File, listener: Option<TcpListener> }

fn try_lock(f: &File) -> bool {
    #[cfg(unix)] { use std::os::unix::io::AsRawFd; unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 } }
    #[cfg(not(unix))] { let _ = f; true }
}

pub enum Outcome { First(Guard), Other }

/// Takes the lock, or (when another Hearth holds it) asks that one to show itself.
pub fn acquire(dir: &Path) -> Outcome {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("hearth.lock");
    #[cfg(windows)]
    let file = { use std::os::windows::fs::OpenOptionsExt; std::fs::OpenOptions::new().write(true).create(true).share_mode(0).open(&path) };
    #[cfg(not(windows))]
    let file = std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(&path);
    match file {
        Ok(f) if try_lock(&f) => {
            let l = TcpListener::bind("127.0.0.1:0").ok();
            if let Some(p) = l.as_ref().and_then(|l| l.local_addr().ok()) { let _ = std::fs::write(dir.join("hearth.port"), p.port().to_string()); }
            Outcome::First(Guard { _lock: f, listener: l })
        }
        _ => { for _ in 0..20 { if wake(dir) { break; } std::thread::sleep(Duration::from_millis(100)); } Outcome::Other }
    }
}

/// Tells the running instance to show its window.
pub fn wake(dir: &Path) -> bool {
    let Some(port) = std::fs::read_to_string(dir.join("hearth.port")).ok().and_then(|s| s.trim().parse::<u16>().ok()) else { return false };
    match TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(500)) { Ok(mut s) => s.write_all(b"show\n").is_ok(), Err(_) => false }
}

impl Guard {
    /// Calls `show` (from a helper thread) whenever another start asks for it.
    pub fn on_wake(&mut self, show: impl Fn() + Send + 'static) {
        let Some(l) = self.listener.take() else { return };
        std::thread::spawn(move || { for s in l.incoming().flatten() { let mut s = s; let _ = s.set_read_timeout(Some(Duration::from_millis(500))); let mut b = [0u8; 16]; if s.read(&mut b).map(|n| &b[..n] == b"show\n").unwrap_or(false) { show(); } } });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    #[test] fn second_start_wakes_the_first() {
        let d = tempfile::tempdir().unwrap();
        let Outcome::First(mut g) = acquire(d.path()) else { panic!("first must win") };
        let (tx, rx) = mpsc::channel(); let tx = std::sync::Mutex::new(tx);
        g.on_wake(move || { let _ = tx.lock().unwrap().send(()); });
        assert!(matches!(acquire(d.path()), Outcome::Other), "second must lose");
        rx.recv_timeout(Duration::from_secs(3)).expect("the first one was told to show");
        drop(g);
    }
    #[test] fn lock_is_free_after_exit() {
        let d = tempfile::tempdir().unwrap();
        { let Outcome::First(_g) = acquire(d.path()) else { panic!() }; }
        assert!(matches!(acquire(d.path()), Outcome::First(_)));
    }
}
