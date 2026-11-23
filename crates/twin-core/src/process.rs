//! Waiting on a child process with a deadline while its output pipes drain.

use std::io::{self, Read};
use std::process::{Child, ExitStatus, Output};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// How long to wait for the output pipes to close once the child has exited:
/// a grandchild that inherited them can hold them open indefinitely
const PIPE_GRACE: Duration = Duration::from_millis(200);

pub(crate) enum Waited {
    Exited(Output),
    /// Killed at the deadline, with whatever it printed before that
    TimedOut(Output),
}

/// Waits for `child` until `timeout`, reading its stdout and stderr on
/// threads so a child that prints more than the pipe buffer holds cannot
/// stall. A pipe the caller did not set to piped reads as empty.
pub(crate) fn wait_with_timeout(mut child: Child, timeout: Duration) -> io::Result<Waited> {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(status) = child.try_wait()? {
            let grace = deadline.saturating_duration_since(Instant::now()) + PIPE_GRACE;
            return Ok(Waited::Exited(collect(status, stdout, stderr, grace)));
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait()?;
            return Ok(Waited::TimedOut(collect(
                status, stdout, stderr, PIPE_GRACE,
            )));
        }

        thread::sleep(Duration::from_millis(25));
    }
}

/// What a pipe has printed so far, and a signal once it closes
struct Pipe {
    buffer: Arc<Mutex<Vec<u8>>>,
    closed: Receiver<()>,
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Option<Pipe> {
    let mut pipe = pipe?;
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let (sender, closed) = mpsc::channel();
    let shared = Arc::clone(&buffer);
    thread::spawn(move || {
        let mut chunk = [0; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    if let Ok(mut buffer) = shared.lock() {
                        buffer.extend_from_slice(&chunk[..read]);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let _ = sender.send(());
    });
    Some(Pipe { buffer, closed })
}

fn collect(
    status: ExitStatus,
    stdout: Option<Pipe>,
    stderr: Option<Pipe>,
    grace: Duration,
) -> Output {
    let until = Instant::now() + grace;
    let read = |pipe: Option<Pipe>| {
        pipe.map(|pipe| {
            let _ = pipe
                .closed
                .recv_timeout(until.saturating_duration_since(Instant::now()));
            pipe.buffer
                .lock()
                .map(|mut buffer| std::mem::take(&mut *buffer))
                .unwrap_or_default()
        })
        .unwrap_or_default()
    };
    Output {
        status,
        stdout: read(stdout),
        stderr: read(stderr),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn spawn(script: &str) -> Child {
        Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("sh should start")
    }

    #[test]
    fn keeps_partial_output_of_a_timed_out_child() {
        let started = Instant::now();
        // `; :` makes sh fork, so the sleep outlives the killed sh
        let child = spawn("echo partial >&2; sleep 5; :");
        let Waited::TimedOut(output) =
            wait_with_timeout(child, Duration::from_millis(300)).expect("wait should work")
        else {
            panic!("a sleeping child should time out");
        };

        assert!(started.elapsed() < Duration::from_millis(1500));
        assert!(!output.status.success());
        assert_eq!(output.stderr, b"partial\n");
    }
}
