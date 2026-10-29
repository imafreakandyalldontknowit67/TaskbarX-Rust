//! Process I/O lives entirely on a worker thread. Only explicit CLI commands
//! are used; no shell, task scheduler, window manipulation, or sibling library.
use crate::protocol::{CommandSpec, Mode};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const CAPTURE_LIMIT: u64 = 64 * 1024;

#[derive(Clone, Debug, Default)]
pub struct ProcessReply {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub pid: Option<u32>,
}
impl ProcessReply {
    pub fn output(&self) -> String {
        let mut s = String::new();
        for (label, data) in [("stdout", &self.stdout), ("stderr", &self.stderr)] {
            if !data.is_empty() {
                s.push_str(&format!("\n{label}:\n{}", String::from_utf8_lossy(data)));
            }
        }
        s
    }
    pub fn success(&self) -> bool {
        !self.timed_out && self.code == Some(0)
    }
}

pub trait Process {
    fn execute(&mut self, spec: &CommandSpec) -> Result<ProcessReply, String>;
    fn poll(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// Regular files avoid pipe deadlocks and, importantly, broken stdout pipes
/// when the configurator closes while a separately running engine continues.
/// Windows delete-on-close cleans up after the LAST (including child) handle.
struct Capture {
    file: File,
    path: PathBuf,
}
impl Capture {
    fn new() -> Result<Self, String> {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "taskbarx-configurator-{}-{}.log",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(0x04000000); // FILE_FLAG_DELETE_ON_CLOSE
            }
            match options.open(&path) {
                Ok(file) => return Ok(Self { file, path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Cannot create engine output capture: {e}")),
            }
        }
        Err("Cannot allocate a unique engine output capture file".into())
    }
    fn stdio(&self) -> Result<Stdio, String> {
        self.file
            .try_clone()
            .map(Stdio::from)
            .map_err(|e| format!("Cannot redirect engine output: {e}"))
    }
    fn snapshot(&self) -> Result<Vec<u8>, String> {
        // Open a separate file description: seeking a cloned handle would
        // change the child's write cursor on Windows and corrupt its output.
        let mut file =
            File::open(&self.path).map_err(|e| format!("Cannot read engine output: {e}"))?;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(CAPTURE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > CAPTURE_LIMIT as usize {
            bytes.truncate(CAPTURE_LIMIT as usize);
            bytes.extend_from_slice(b"\n[output truncated at 64 KiB]\n");
        }
        Ok(bytes)
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        #[cfg(not(windows))]
        let _ = std::fs::remove_file(&self.path);
    }
}

struct Runner {
    child: Child,
    stdout: Capture,
    stderr: Capture,
    last: String,
}
#[derive(Default)]
pub struct SystemProcess {
    runners: Vec<Runner>,
}

impl Process for SystemProcess {
    fn execute(&mut self, spec: &CommandSpec) -> Result<ProcessReply, String> {
        if !spec.program.is_absolute() {
            return Err("Refusing a non-absolute engine path (PATH lookup is not allowed)".into());
        }
        if !spec.program.is_file() {
            return Err(format!("Engine missing or inaccessible: {}. Put taskbarx.exe beside the configurator or pass --engine PATH.", spec.program.display()));
        }
        let stdout = Capture::new()?;
        let stderr = Capture::new()?;
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(if spec.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(stdout.stdio()?)
            .stderr(stderr.stdio()?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW, including run
        }
        let mut child = command.spawn().map_err(|e| {
            format!(
                "Could not execute {} ({}): {e}",
                spec.program.display(),
                spec.label()
            )
        })?;
        let started = Instant::now();
        let input = if let Some(data) = &spec.stdin {
            let data = data.clone();
            let mut pipe = child
                .stdin
                .take()
                .ok_or("Engine stdin pipe was not created")?;
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                let result = pipe.write_all(&data).map_err(|e| e.to_string());
                drop(pipe);
                let _ = tx.send(result);
            });
            Some(rx)
        } else {
            None
        };
        let mut reply = ProcessReply::default();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    reply.code = status.code();
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    // Never forcibly kill a runtime because monitoring failed.
                    if spec.mode == Mode::Wait {
                        let _ = child.kill();
                    }
                    return Err(format!(
                        "Cannot wait for {}: {e}; runtime state is unknown",
                        spec.label()
                    ));
                }
            }
            if started.elapsed() >= spec.timeout {
                if spec.mode == Mode::Launch {
                    reply.pid = Some(child.id());
                } else {
                    reply.timed_out = true;
                    // Only the short-lived CLI request is terminated, never an
                    // existing foreground runtime. Do not block on wait().
                    let _ = child.kill();
                    let _ = child.try_wait();
                }
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let capture_error = |error| {
            format!("{} process {} was spawned, but output capture failed: {error}. Exit: {:?}; timed out: {}; runtime may still be running.", spec.label(), child.id(), reply.code, reply.timed_out)
        };
        reply.stdout = stdout.snapshot().map_err(capture_error)?;
        reply.stderr = stderr.snapshot().map_err(capture_error)?;
        if reply.pid.is_some() {
            self.runners.push(Runner {
                child,
                stdout,
                stderr,
                last: reply.output(),
            });
        }
        if let Some(rx) = input {
            if !reply.timed_out {
                match rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        return Err(format!(
                            "Could not send settings to engine stdin: {e}{}",
                            reply.output()
                        ))
                    }
                    Err(_) => {
                        return Err(format!(
                            "Engine stdin did not complete within the deadline{}",
                            reply.output()
                        ))
                    }
                }
            }
        }
        Ok(reply)
    }

    fn poll(&mut self) -> Vec<String> {
        let mut messages = Vec::new();
        self.runners.retain_mut(|runner| {
            let output = (|| -> Result<String, String> {
                Ok(ProcessReply { stdout: runner.stdout.snapshot()?, stderr: runner.stderr.snapshot()?, ..Default::default() }.output())
            })();
            match runner.child.try_wait() {
                Ok(Some(status)) => {
                    messages.push(format!("Engine process {} exited ({status}). This is not a confirmation of taskbar support or restoration.{}", runner.child.id(), output.unwrap_or_else(|e| e)));
                    false
                }
                Ok(None) => {
                    match output {
                        Ok(s) if s != runner.last => {
                            messages.push(format!("Engine process {} output (not a support check):{s}", runner.child.id()));
                            runner.last = s;
                        }
                        Err(e) if runner.last != e => { messages.push(e.clone()); runner.last = e; }
                        _ => {}
                    }
                    true
                }
                Err(e) => { messages.push(format!("Engine monitor error: {e}; runtime may still be running.")); false }
            }
        });
        messages
    }
}
