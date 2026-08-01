use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use super::PreviewCancellation;

pub(super) const MAX_PREVIEW_OUTPUT_BYTES: usize = 1024 * 1024;
pub(super) const PREVIEW_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(super) struct BoundedProcessOutput {
    pub(super) status: Option<ExitStatus>,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) truncated: bool,
    pub(super) timed_out: bool,
    pub(super) cancelled: bool,
}

pub(super) fn run_bounded_process(
    process: &mut Command,
    cancellation: &PreviewCancellation,
) -> std::io::Result<BoundedProcessOutput> {
    process.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    process.process_group(0);

    let mut child = process.spawn()?;
    let limit_hit = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take().expect("preview stdout is piped");
    let stderr = child.stderr.take().expect("preview stderr is piped");
    let stdout_reader = spawn_bounded_reader(stdout, limit_hit.clone());
    let stderr_reader = spawn_bounded_reader(stderr, limit_hit.clone());
    let started = Instant::now();
    let mut timed_out = false;
    let mut cancelled = false;
    let mut truncated = false;

    let status = loop {
        if cancellation.is_cancelled() {
            cancelled = true;
            terminate_process_tree(&mut child);
            break child.wait().ok();
        }
        if limit_hit.load(Ordering::Acquire) {
            truncated = true;
            terminate_process_tree(&mut child);
            break child.wait().ok();
        }
        if started.elapsed() >= PREVIEW_COMMAND_TIMEOUT {
            timed_out = true;
            terminate_process_tree(&mut child);
            break child.wait().ok();
        }
        if let Some(status) = child.try_wait()? {
            // A preview shell may have left background descendants holding the
            // capture pipes open. Tear down its dedicated process group before
            // joining the reader threads.
            terminate_process_tree(&mut child);
            break Some(status);
        }
        thread::sleep(PROCESS_POLL_INTERVAL);
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    truncated |= limit_hit.load(Ordering::Acquire);
    Ok(BoundedProcessOutput {
        status,
        stdout,
        stderr,
        truncated,
        timed_out,
        cancelled,
    })
}

fn spawn_bounded_reader<R>(mut reader: R, limit_hit: Arc<AtomicBool>) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::with_capacity(16 * 1024);
        let mut buffer = [0u8; 8192];
        while output.len() < MAX_PREVIEW_OUTPUT_BYTES {
            let remaining = MAX_PREVIEW_OUTPUT_BYTES - output.len();
            let read_len = remaining.min(buffer.len());
            match reader.read(&mut buffer[..read_len]) {
                Ok(0) | Err(_) => return output,
                Ok(read) => output.extend_from_slice(&buffer[..read]),
            }
        }

        let mut probe = [0u8; 1];
        if reader.read(&mut probe).is_ok_and(|read| read > 0) {
            limit_hit.store(true, Ordering::Release);
        }
        output
    })
}

fn terminate_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let process_group = -(child.id() as i32);
        // The child was placed in its own process group before spawn.
        unsafe {
            libc::kill(process_group, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn process_output_is_bounded_and_terminated() {
        let mut command = Command::new("sh");
        command.args(["-c", "yes preview"]);
        let started = Instant::now();
        let output = run_bounded_process(&mut command, &PreviewCancellation::default()).unwrap();

        assert!(output.truncated);
        assert!(output.stdout.len() <= MAX_PREVIEW_OUTPUT_BYTES);
        assert!(output.stderr.len() <= MAX_PREVIEW_OUTPUT_BYTES);
        assert!(started.elapsed() < PREVIEW_COMMAND_TIMEOUT);
    }

    #[test]
    fn cancellation_terminates_a_process_group_promptly() {
        let cancellation = PreviewCancellation::default();
        let worker_cancellation = cancellation.clone();
        let handle = thread::spawn(move || {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 30 & wait"]);
            run_bounded_process(&mut command, &worker_cancellation).unwrap()
        });

        thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        cancellation.cancel();
        let output = handle.join().unwrap();

        assert!(output.cancelled);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
