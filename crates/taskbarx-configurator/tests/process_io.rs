//! Real adapter tests launch ONLY this test executable, never taskbarx.exe.
//! Ignored fixture tests are selected explicitly by its child test harness.
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};
use taskbarx_configurator::{
    process::{Process, SystemProcess},
    protocol::{CommandSpec, Mode},
};

fn fixture(name: &str, mode: Mode, timeout: Duration) -> CommandSpec {
    CommandSpec {
        program: std::env::current_exe().unwrap(),
        args: ["--exact", name, "--ignored", "--nocapture"]
            .map(Into::into)
            .into(),
        stdin: None,
        mode,
        timeout,
    }
}

#[test]
#[ignore = "subprocess fixture, not a standalone test"]
fn echo_fixture() {
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    std::io::stdout().write_all(&input).unwrap();
    std::io::stderr()
        .write_all(b"fixture stderr \xff\n")
        .unwrap();
}

#[test]
#[ignore = "subprocess fixture, not a standalone test"]
fn sleeping_fixture() {
    std::io::stderr().write_all(b"fixture starting\n").unwrap();
    std::thread::sleep(Duration::from_secs(2));
    std::io::stdout().write_all(b"fixture finished\n").unwrap();
}

#[test]
#[ignore = "subprocess fixture, not a standalone test"]
fn failing_fixture() {
    std::io::stdout().write_all(b"partial result\n").unwrap();
    std::io::stderr()
        .write_all(b"deliberate fixture failure\n")
        .unwrap();
    std::process::exit(17);
}

#[test]
#[ignore = "subprocess fixture, not a standalone test"]
fn verbose_fixture() {
    std::io::stdout()
        .write_all(&vec![b'x'; 256 * 1024])
        .unwrap();
    std::io::stderr()
        .write_all(&vec![b'e'; 256 * 1024])
        .unwrap();
}

#[test]
#[ignore = "subprocess fixture, not a standalone test"]
fn detached_fixture() {
    let mut marker = String::new();
    std::io::stdin().read_to_string(&mut marker).unwrap();
    std::thread::sleep(Duration::from_millis(600));
    // Both writes must still work after the parent drops its child/log handles.
    std::io::stdout()
        .write_all(b"stdout after parent detached\n")
        .unwrap();
    std::io::stderr()
        .write_all(b"stderr after parent detached\n")
        .unwrap();
    std::fs::write(marker, b"completed").unwrap();
}

#[test]
fn real_adapter_pipes_stdin_and_captures_both_output_streams() {
    let mut spec = fixture("echo_fixture", Mode::Wait, Duration::from_secs(5));
    let input = b"{\"primary_offset\":-89,\"path\":\"a b & c\"}".to_vec();
    spec.stdin = Some(input.clone());
    let reply = SystemProcess::default().execute(&spec).unwrap();
    assert!(reply.success());
    assert!(reply
        .stdout
        .windows(input.len())
        .any(|window| window == input));
    assert!(reply.stderr.starts_with(b"fixture stderr \xff"));
    assert!(reply.output().contains("fixture stderr"));
}

#[test]
fn real_adapter_has_bounded_timeout() {
    let spec = fixture("sleeping_fixture", Mode::Wait, Duration::from_millis(250));
    let start = Instant::now();
    let reply = SystemProcess::default().execute(&spec).unwrap();
    assert!(reply.timed_out);
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(reply.output().contains("fixture starting"));
}

#[test]
fn actual_nonzero_exit_preserves_both_streams() {
    let reply = SystemProcess::default()
        .execute(&fixture(
            "failing_fixture",
            Mode::Wait,
            Duration::from_secs(5),
        ))
        .unwrap();
    assert_eq!(reply.code, Some(17));
    assert!(!reply.success());
    assert!(reply.output().contains("partial result"));
    assert!(reply.output().contains("deliberate fixture failure"));
}

#[test]
fn large_output_is_bounded_without_pipe_deadlock() {
    let reply = SystemProcess::default()
        .execute(&fixture(
            "verbose_fixture",
            Mode::Wait,
            Duration::from_secs(5),
        ))
        .unwrap();
    assert!(reply.success());
    assert!(reply.stdout.len() < 66000 && reply.stderr.len() < 66000);
    assert!(reply.output().contains("output truncated"));
}

#[test]
fn real_executable_path_with_spaces_is_not_shell_parsed() {
    let dir = std::env::temp_dir().join(format!(
        "taskbarx configurator fixture & {}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test process.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
    let mut spec = fixture("echo_fixture", Mode::Wait, Duration::from_secs(5));
    spec.program = path.clone();
    spec.stdin = Some(b"spaced executable".to_vec());
    let result = SystemProcess::default().execute(&spec);
    // Only these exact test-generated objects are removed, never recursively.
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_dir(dir);
    assert!(result.unwrap().output().contains("spaced executable"));
}

#[test]
fn closing_process_owner_does_not_stop_a_launched_runtime_or_break_its_output() {
    let marker = std::env::temp_dir().join(format!(
        "taskbarx-detached-fixture-{}.txt",
        std::process::id()
    ));
    assert!(!marker.exists());
    let mut spec = fixture("detached_fixture", Mode::Launch, Duration::from_millis(100));
    spec.stdin = Some(marker.to_string_lossy().as_bytes().to_vec());
    let mut process = SystemProcess::default();
    assert!(process.execute(&spec).unwrap().pid.is_some());
    drop(process);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() {
        assert!(
            Instant::now() < deadline,
            "detached fixture failed to finish"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(std::fs::read(&marker).unwrap(), b"completed");
    std::fs::remove_file(marker).unwrap();
}

#[test]
fn launch_observation_does_not_kill_runner_and_later_exit_is_reported() {
    let spec = fixture("sleeping_fixture", Mode::Launch, Duration::from_millis(250));
    let mut process = SystemProcess::default();
    let reply = process.execute(&spec).unwrap();
    assert!(reply.pid.is_some());
    assert!(!reply.timed_out);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let reports = process.poll();
        if reports.iter().any(|s| s.contains("exited")) {
            assert!(reports.iter().any(|s| s.contains("fixture finished")));
            break;
        }
        assert!(Instant::now() < deadline, "fixture failed to exit");
        std::thread::sleep(Duration::from_millis(25));
    }
}
