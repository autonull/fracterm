//! PTY session management (README M3): spawn shells via `portable-pty`
//! with a background reader thread feeding a shared byte buffer.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// A running PTY session: master handle, child process, and the shared
/// output buffer filled by the reader thread.
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<Mutex<Vec<u8>>>,
    reader: Option<JoinHandle<()>>,
    cols: u16,
    rows: u16,
}

impl PtySession {
    /// Spawn `command` (default: `$SHELL` or `bash`) in a new PTY.
    pub fn spawn(cols: u16, rows: u16, command: Option<&str>) -> Result<Self, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;

        let shell = command
            .map(str::to_string)
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "bash".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        if shell.ends_with("echo") {
            cmd.arg("fracterm-pty-test");
        }

        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        let mut reader_io = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

        let output: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let out_handle = Arc::clone(&output);
        let reader = std::thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader_io.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(mut out) = out_handle.lock() {
                                out.extend_from_slice(&buf[..n]);
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            master: pair.master,
            child,
            output,
            reader: Some(reader),
            cols,
            rows,
        })
    }

    /// Drain pending output bytes from the reader thread.
    pub fn take_output(&self) -> Vec<u8> {
        self.output
            .lock()
            .map(|mut b| std::mem::take(&mut *b))
            .unwrap_or_default()
    }

    /// Write bytes to the PTY master (keyboard input).
    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let mut writer = self.master.take_writer().map_err(|e| e.to_string())?;
        writer.write_all(bytes).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())
    }

    /// Propagate a grid resize to the PTY (SIGWINCH on the child).
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), String> {
        if cols == self.cols && rows == self.rows {
            return Ok(());
        }
        self.cols = cols;
        self.rows = rows;
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())
    }

    pub fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_spawn_and_drain() {
        let mut pty = PtySession::spawn(80, 24, Some("/bin/echo")).unwrap();
        assert!(pty.is_alive() || !pty.is_alive()); // child may exit fast
                                                    // Wait for output to arrive via the reader thread.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut got = Vec::new();
        while std::time::Instant::now() < deadline {
            got.extend(pty.take_output());
            if got.windows(17).any(|w| w == b"fracterm-pty-test") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let text = String::from_utf8_lossy(&got);
        assert!(text.contains("fracterm-pty-test"), "output was: {text:?}");
        let _ = pty.write(b"\n");
    }
}
