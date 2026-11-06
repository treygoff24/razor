//! The terminal's output: one queue of whole frames and picture rows, in the order they were
//! made, written to the terminal's own descriptor without blocking, so a terminal that reads
//! slowly or not at all never keeps the loop from reading input. A partial write splits only the
//! queue's bytes, so nothing is ever written inside an escape another left unfinished.
use std::{
    ffi::CStr, fs::File, io, io::Write, os::unix::ffi::OsStrExt, os::unix::fs::OpenOptionsExt,
};
use tokio::io::{Interest, unix::AsyncFd};

pub struct Writer {
    tty: AsyncFd<File>,
    queue: Vec<u8>,
    written: usize,
}

impl Writer {
    /// The terminal opened again, so that only this descriptor does not block: standard output's
    /// is shared with the shell and stays as it was.
    pub fn new() -> io::Result<Self> {
        let mut name = [0; 4096];
        // SAFETY: `name` is writable for its full length; ttyname_r terminates a successful result.
        let error = unsafe { libc::ttyname_r(libc::STDOUT_FILENO, name.as_mut_ptr(), name.len()) };
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
        // SAFETY: ttyname_r succeeded, so `name` contains a nul-terminated path.
        let name = unsafe { CStr::from_ptr(name.as_ptr()) };
        let tty = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
            .open(std::ffi::OsStr::from_bytes(name.to_bytes()))?;
        // SAFETY: the file owns its descriptor, open and the same for as long as `tty` holds it.
        let tty = unsafe { AsyncFd::register_with_interest(tty, Interest::WRITABLE)? };
        let (queue, written) = (Vec::new(), 0);
        Ok(Self {
            tty,
            queue,
            written,
        })
    }

    /// Everything queued has been written: the loop may queue the next frame or row.
    pub fn idle(&self) -> bool {
        self.written == self.queue.len()
    }

    /// Waits until the terminal takes more of the queue, and writes what it takes.
    pub async fn drain(&mut self) -> io::Result<()> {
        let n = {
            let mut ready = match self.tty.writable().await {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(()),
                ready => ready?,
            };
            let rest = &self.queue[self.written..];
            // A write the terminal refuses clears the readiness, so the next wait is a real one.
            match ready.try_io(|tty| (&mut tty.get_ref()).write(rest)) {
                Ok(Ok(n)) => n,
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => 0,
                Ok(Err(e)) => return Err(e),
                Err(_) => 0,
            }
        };
        self.wrote(n);
        Ok(())
    }

    /// Writes the rest through standard output, waiting as long as the terminal takes: before the
    /// app leaves the alternate screen or opens the editor, which write there themselves.
    pub fn finish(&mut self) -> io::Result<()> {
        let mut out = io::stdout();
        out.write_all(&self.queue[self.written..])?;
        self.wrote(self.queue.len() - self.written);
        out.flush()
    }

    fn wrote(&mut self, n: usize) {
        self.written += n;
        if self.idle() {
            self.queue.clear();
            self.written = 0;
        }
    }
}

impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.queue.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// Writes what the terminal takes now; `drain` writes the rest when it takes more.
    fn flush(&mut self) -> io::Result<()> {
        while !self.idle() {
            match (&mut self.tty.get_ref()).write(&self.queue[self.written..]) {
                Ok(0) => return Ok(()),
                Ok(n) => self.wrote(n),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
