//! Regression tests for CLI lifecycle exit semantics.
//!
//! Contract under test:
//! - `cope start` when stopped        -> exit 0, "COPE started."
//! - `cope start` when already running -> exit 0, "COPE is already running."
//!   ("Already running" is an expected idempotent state, NOT an error.)
//! - `cope stop` when already stopped  -> exit 0, informational message.
//! - genuine startup failure           -> non-zero exit + error output.
//!
//! These tests use COPE_TEST_DATA_DIR for isolated temporary state so they
//! never touch the user's real %LOCALAPPDATA%\COPE installation.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn cope_exe() -> &'static str {
    env!("CARGO_BIN_EXE_cope")
}

struct TestEnv {
    temp_dir: TempDir,
    daemon_child: Option<Child>,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        std::env::set_var("COPE_TEST_DATA_DIR", temp_dir.path());
        std::env::set_var("COPE_TEST_DAEMON_MODE", "ready");
        Self {
            temp_dir,
            daemon_child: None,
        }
    }

    fn config_dir(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    fn pid_file(&self) -> PathBuf {
        self.config_dir().join("daemon.pid")
    }

    fn installed_exe(&self) -> PathBuf {
        self.config_dir().join("cope.exe")
    }

    /// Ensure the test cope.exe is installed in the test data directory.
    fn ensure_installed_exe(&self) {
        let src = PathBuf::from(cope_exe());
        let dst = self.installed_exe();
        std::fs::copy(&src, &dst).expect("failed to copy cope.exe to test data dir");
    }

    fn read_pid(&self) -> Option<u32> {
        std::fs::read_to_string(self.pid_file())
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }

    fn pid_alive(&self, pid: u32) -> bool {
        if pid == 0 {
            return false;
        }
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
        let Ok(handle) = handle else { return false };
        let mut exit_code = 0u32;
        let alive =
            unsafe { GetExitCodeProcess(handle, &mut exit_code).is_ok() } && exit_code == 259;
        let _ = unsafe { CloseHandle(handle) };
        alive
    }

    fn is_daemon_running(&self) -> bool {
        self.read_pid()
            .map(|pid| self.pid_alive(pid))
            .unwrap_or(false)
    }

    /// Start the deterministic lifecycle daemon and wait for it to be ready.
    fn start_daemon(&mut self) -> u32 {
        self.ensure_installed_exe();
        let child = Command::new(self.installed_exe())
            .arg("daemon")
            .env("COPE_TEST_DATA_DIR", self.temp_dir.path())
            .env("COPE_TEST_DAEMON_MODE", "ready")
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn COPE daemon");

        let pid = child.id();
        self.daemon_child = Some(child);

        // Wait for daemon readiness with bounded polling (max 5s).
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if self.is_daemon_running() {
                return pid;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        // If daemon didn't start, capture its stderr for debugging
        if let Some(mut child) = self.daemon_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        panic!("COPE daemon did not confirm startup within timeout");
    }

    fn stop_daemon(&mut self) {
        if let Some(mut child) = self.daemon_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Also ensure PID file is cleaned up
        let _ = std::fs::remove_file(self.pid_file());
    }

    fn verify_single_daemon(&self, expected_pid: u32) {
        let pid = self.read_pid().expect("PID file should exist");
        assert_eq!(pid, expected_pid, "PID file should contain the daemon PID");
        assert!(self.pid_alive(pid), "daemon process should be alive");
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        self.stop_daemon();
        // TempDir cleanup happens automatically
    }
}

impl TestEnv {
    fn run_cope(&self, arg: &str) -> (Option<i32>, String, String) {
        let stdout_path = self.temp_dir.path().join(format!("cope-{arg}-stdout.txt"));
        let stderr_path = self.temp_dir.path().join(format!("cope-{arg}-stderr.txt"));
        let stdout_file =
            std::fs::File::create(&stdout_path).expect("failed to create stdout capture");
        let stderr_file =
            std::fs::File::create(&stderr_path).expect("failed to create stderr capture");
        let mut child = Command::new(cope_exe())
            .arg(arg)
            .env("COPE_TEST_DATA_DIR", self.temp_dir.path())
            .env("COPE_TEST_DAEMON_MODE", "ready")
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .spawn()
            .expect("failed to spawn cope.exe");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return (
                        status.code(),
                        std::fs::read_to_string(&stdout_path).unwrap_or_default(),
                        std::fs::read_to_string(&stderr_path).unwrap_or_default(),
                    );
                }
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("child process did not exit within 5 seconds");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("try_wait failed: {}", e);
                }
            }
        }
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for e in chars.by_ref() {
                if e.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn stop_while_stopped_exits_zero() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let env = TestEnv::new();

    // Write a stale PID file (nonexistent process)
    std::fs::write(env.pid_file(), "999999").unwrap();

    let (code, stdout, stderr) = env.run_cope("stop");
    assert_eq!(code, Some(0), "stop while stopped must exit 0");
    assert!(
        strip_ansi(&stdout).contains("COPE is not running."),
        "expected informational message, got stdout={:?} stderr={:?}",
        stdout,
        stderr
    );
}

#[test]
fn start_when_already_running_exits_zero_and_does_not_spawn() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut env = TestEnv::new();
    env.ensure_installed_exe();

    // Start the deterministic lifecycle daemon
    let pid = env.start_daemon();

    // Verify exactly one daemon exists
    env.verify_single_daemon(pid);

    // First start: daemon already running -> exit 0, informational
    let (code1, stdout1, stderr1) = env.run_cope("start");
    assert_eq!(
        code1,
        Some(0),
        "start while already running must exit 0 (got stderr={:?})",
        stderr1
    );
    assert!(
        strip_ansi(&stdout1).contains("COPE is already running."),
        "expected 'already running' message, got {:?}",
        stdout1
    );

    // Second consecutive start: same idempotent outcome (singleton holds)
    let (code2, stdout2, _) = env.run_cope("start");
    assert_eq!(code2, Some(0));
    assert!(strip_ansi(&stdout2).contains("COPE is already running."));

    // Exactly one daemon remains
    env.verify_single_daemon(pid);

    // Third rapid start: still idempotent
    let (code3, stdout3, _) = env.run_cope("start");
    assert_eq!(code3, Some(0));
    assert!(strip_ansi(&stdout3).contains("COPE is already running."));
    env.verify_single_daemon(pid);
}

#[test]
fn start_genuine_failure_exits_nonzero() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let env = TestEnv::new();

    // No installed exe anywhere -> spawn fails -> genuine startup failure
    // (TestEnv uses isolated temp dir, so no cope.exe exists there)
    let (code, stdout, stderr) = env.run_cope("start");
    assert_ne!(
        code,
        Some(0),
        "genuine startup failure must exit non-zero (stdout={:?})",
        stdout
    );
    assert!(
        !stderr.trim().is_empty(),
        "genuine failure must report an error on stderr"
    );
}

#[test]
fn start_when_stopped_exits_zero_and_spawns() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let env = TestEnv::new();
    env.ensure_installed_exe();

    // Ensure no daemon is running
    let _ = std::fs::remove_file(env.pid_file());

    // First start: should spawn daemon
    let (code1, stdout1, stderr1) = env.run_cope("start");
    assert_eq!(
        code1,
        Some(0),
        "start when stopped must exit 0 (got stderr={:?})",
        stderr1
    );
    assert!(
        strip_ansi(&stdout1).contains("COPE started."),
        "expected 'COPE started.' message, got {:?}",
        stdout1
    );

    // Verify daemon is now running
    let pid = env.read_pid().expect("PID file should exist after start");
    assert!(env.pid_alive(pid), "daemon should be alive after start");
    env.verify_single_daemon(pid);

    // Second start: should be idempotent
    let (code2, stdout2, _) = env.run_cope("start");
    assert_eq!(code2, Some(0));
    assert!(strip_ansi(&stdout2).contains("COPE is already running."));
    env.verify_single_daemon(pid);

    // Stop daemon
    let (code3, stdout3, _) = env.run_cope("stop");
    assert_eq!(code3, Some(0));
    assert!(strip_ansi(&stdout3).contains("COPE stopped."));

    // Verify daemon is gone
    assert!(
        !env.pid_file().exists(),
        "PID file should be removed after stop"
    );
    assert!(
        !env.is_daemon_running(),
        "daemon should not be running after stop"
    );
}
