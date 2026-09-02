//! Mirror this process's stdout/stderr to an append-only log file.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub struct StdioTee {
    stdout: StreamTee,
    stderr: StreamTee,
}

struct StreamTee {
    target: RawFd,
    saved: Option<OwnedFd>,
    thread: Option<JoinHandle<()>>,
}

impl StreamTee {
    fn idle() -> Self {
        Self {
            target: -1,
            saved: None,
            thread: None,
        }
    }

    fn restore(&mut self) {
        if let Some(saved) = self.saved.as_ref() {
            unsafe {
                libc::dup2(saved.as_raw_fd(), self.target);
            }
        }
        self.saved.take();
    }

    fn join(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for StdioTee {
    fn drop(&mut self) {
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
        self.stdout.restore();
        self.stderr.restore();
        self.stdout.join();
        self.stderr.join();
    }
}

fn dup_owned(fd: RawFd) -> io::Result<OwnedFd> {
    let n = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if n < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(n) })
    }
}

fn pipe_cloexec() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

fn tee_one(target: RawFd, log: Arc<Mutex<File>>) -> io::Result<StreamTee> {
    let saved = dup_owned(target)?;
    let (read_fd, write_fd) = pipe_cloexec()?;
    if unsafe { libc::dup2(write_fd.as_raw_fd(), target) } < 0 {
        return Err(io::Error::last_os_error());
    }
    drop(write_fd);

    let restore = |saved: &OwnedFd| unsafe {
        libc::dup2(saved.as_raw_fd(), target);
    };

    let mut reader = File::from(read_fd);
    let mut console = match dup_owned(saved.as_raw_fd()) {
        Ok(fd) => File::from(fd),
        Err(e) => {
            restore(&saved);
            return Err(e);
        }
    };
    let thread = match thread::Builder::new()
        .name(format!("neo-tee-{target}"))
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = &buf[..n];
                        let _ = console.write_all(chunk);
                        let _ = console.flush();
                        if let Ok(mut log) = log.lock() {
                            let _ = log.write_all(chunk);
                            let _ = log.flush();
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        }) {
        Ok(t) => t,
        Err(e) => {
            restore(&saved);
            return Err(e);
        }
    };

    Ok(StreamTee {
        target,
        saved: Some(saved),
        thread: Some(thread),
    })
}

/// Copy stdout and stderr to `log_path` (append) while still writing to the original console.
pub fn tee_stdio_to_log(log_path: &Path) -> io::Result<StdioTee> {
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let log = Arc::new(Mutex::new(log));
    let stdout = tee_one(libc::STDOUT_FILENO, Arc::clone(&log))?;
    let stderr = tee_one(libc::STDERR_FILENO, log).unwrap_or_else(|_| StreamTee::idle());
    Ok(StdioTee { stdout, stderr })
}
