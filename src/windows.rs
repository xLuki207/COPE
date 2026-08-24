use crate::hotkeys::HotkeyManager;
use anyhow::Result;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::{fs, io};
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use winreg::enums::*;
use winreg::RegKey;

const APP_NAME: &str = "COPE";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const GLOBAL_MUTEX_NAME: &str = "Global\\COPE_SINGLE_INSTANCE_MUTEX";

fn global_mutex_name() -> String {
    if let Ok(test_dir) = std::env::var("COPE_TEST_DATA_DIR") {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        test_dir.hash(&mut hasher);
        return format!("Global\\COPE_TEST_MUTEX_{:x}", hasher.finish());
    }
    GLOBAL_MUTEX_NAME.to_string()
}

fn test_daemon_mode() -> bool {
    std::env::var("COPE_TEST_DAEMON_MODE").as_deref() == Ok("ready")
}

pub fn startup_command(exe_path: &std::path::Path) -> String {
    format!("\"{}\" daemon", exe_path.display())
}

pub fn enable_startup() -> Result<()> {
    let exe_path = installed_exe_path()?;
    let cmd = startup_command(&exe_path);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
    run_key.set_value(APP_NAME, &cmd)?;
    Ok(())
}

pub fn disable_startup() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(run_key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) else {
        return Ok(());
    };
    let _ = run_key.delete_value(APP_NAME);
    Ok(())
}

pub fn is_startup_enabled() -> Result<bool> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ)?;
    let value: String = match run_key.get_value(APP_NAME) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    Ok(value.trim_end().ends_with("daemon"))
}

#[allow(dead_code)]
pub fn current_exe_path() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(exe)
}

pub fn installed_exe_path() -> Result<PathBuf> {
    let dir = crate::config::config_dir()?;
    Ok(dir.join("cope.exe"))
}

pub fn installed_cope_dir() -> Result<PathBuf> {
    crate::config::config_dir()
}

const OWNED_DATA_FILES: [&str; 3] = ["config.json", "history.jsonl", "daemon.pid"];
const CLEANUP_COMMAND: &str = "__cope_cleanup";

/// Return the canonical COPE directory, refusing to follow a redirected path
/// outside the configured per-user location.
pub fn verified_cope_dir() -> Result<PathBuf> {
    let configured = crate::config::configured_data_dir()?;
    let canonical = fs::canonicalize(&configured)?;

    if std::env::var_os("COPE_TEST_DATA_DIR").is_none() {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("LOCALAPPDATA is not available"))?;
        let expected = fs::canonicalize(local_app_data)?.join("COPE");
        if !same_path(&canonical, &expected) {
            anyhow::bail!(
                "Refusing to clean redirected COPE data directory {}",
                canonical.display()
            );
        }
    }

    Ok(canonical)
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

pub fn remove_owned_data_files(dir: &std::path::Path) -> Result<()> {
    for name in OWNED_DATA_FILES {
        let path = dir.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "Failed to remove COPE-owned file {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Remove COPE's directory only when it is empty. User-created siblings are
/// deliberately retained.
pub fn remove_cope_dir_if_empty(dir: &std::path::Path) -> Result<bool> {
    if !dir.exists() {
        return Ok(false);
    }

    if fs::read_dir(dir)?.next().is_some() {
        return Ok(false);
    }

    fs::remove_dir(dir)?;
    Ok(true)
}

fn cleanup_helper_path() -> Result<PathBuf> {
    let current_exe = current_exe_path()?;
    let pid = std::process::id();
    let temp_dir = std::env::temp_dir();

    for suffix in 0..100u32 {
        let path = temp_dir.join(format!("cope-uninstall-{pid}-{suffix}.exe"));
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };

        let mut input = fs::File::open(&current_exe)?;
        io::copy(&mut input, &mut output)?;
        io::Write::flush(&mut output)?;
        return Ok(path);
    }

    anyhow::bail!("Could not allocate a unique COPE cleanup helper path")
}

/// Schedule deletion of the running installed executable and cleanup of its
/// data directory. The helper is a private copy of COPE in the user temp
/// directory and exits after the parent releases its image; a bounded Windows
/// cleanup process then removes the helper itself.
pub fn schedule_deferred_cleanup(installed_exe: &std::path::Path) -> Result<()> {
    if std::env::var("COPE_TEST_FAIL_UNINSTALL_SCHEDULE").as_deref() == Ok("1") {
        anyhow::bail!("test failure: deferred uninstall cleanup was not scheduled");
    }

    let current_exe = current_exe_path()?;
    let current_canonical = fs::canonicalize(current_exe)?;
    let installed_canonical = fs::canonicalize(installed_exe)?;

    if !same_path(&current_canonical, &installed_canonical) {
        fs::remove_file(installed_exe)?;
        return Ok(());
    }

    let helper_path = cleanup_helper_path()?;
    let child_result = Command::new(&helper_path)
        .arg(CLEANUP_COMMAND)
        .arg(std::process::id().to_string())
        .creation_flags(0x08000000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match child_result {
        Ok(_) => {}
        Err(error) => {
            let _ = fs::remove_file(&helper_path);
            return Err(error.into());
        }
    };

    Ok(())
}

fn wait_for_parent_exit(parent_pid: u32) -> Result<()> {
    use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };

    let handle = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            parent_pid,
        )
    };
    match handle {
        Ok(handle) if !handle.is_invalid() => {
            if verify_handle_is_cope_daemon(handle) {
                let wait_result = unsafe { WaitForSingleObject(handle, 30_000) };
                if wait_result == WAIT_TIMEOUT {
                    let _ = unsafe { CloseHandle(handle) };
                    anyhow::bail!("COPE uninstall helper timed out waiting for its parent");
                }
                if wait_result != WAIT_OBJECT_0 {
                    let _ = unsafe { CloseHandle(handle) };
                    anyhow::bail!("COPE uninstall helper could not wait for its parent");
                }
            }
            let _ = unsafe { CloseHandle(handle) };
        }
        _ => {
            // The parent may have exited between spawning this helper and
            // opening its process handle. No wait is needed in that case.
        }
    }
    Ok(())
}

fn schedule_helper_self_delete(path: &std::path::Path) -> Result<()> {
    let temp_dir = fs::canonicalize(std::env::temp_dir())?;
    let canonical_path = fs::canonicalize(path)?;
    if canonical_path.parent() != Some(temp_dir.as_path())
        || !canonical_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("cope-uninstall-") && name.ends_with(".exe"))
    {
        anyhow::bail!(
            "Refusing to schedule deletion of unverified COPE cleanup helper {}",
            canonical_path.display()
        );
    }

    let path_string = canonical_path.to_string_lossy();
    let command_path = path_string.strip_prefix(r"\\?\").unwrap_or(&path_string);
    if !command_path.chars().all(|character| {
        character.is_ascii_alphanumeric()
            || matches!(character, ':' | '\\' | '/' | '_' | '-' | '.' | ' ' | '~')
    }) {
        anyhow::bail!(
            "Refusing to pass unsafe COPE cleanup helper path to Windows cleanup: {}",
            canonical_path.display()
        );
    }

    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("SystemRoot is not available"))?;
    if !system_root.to_string_lossy().chars().all(|character| {
        character.is_ascii_alphanumeric()
            || matches!(character, ':' | '\\' | '/' | '_' | '-' | '.' | ' ' | '~')
    }) {
        anyhow::bail!("Refusing unsafe Windows system path for cleanup");
    }
    let cmd = system_root.join("System32").join("cmd.exe");
    let ping = system_root.join("System32").join("ping.exe");
    if !cmd.is_file() || !ping.is_file() {
        anyhow::bail!("Windows cleanup tools are unavailable");
    }
    let cleanup_attempt = format!(
        "\"{}\" 127.0.0.1 -n 2 > nul & del /f /q \"{}\"",
        ping.display(),
        command_path
    );
    let command_line = (0..5)
        .map(|_| cleanup_attempt.as_str())
        .collect::<Vec<_>>()
        .join(" & ");
    Command::new(&cmd)
        // `cmd.exe` does not understand the backslash quote escaping that
        // Rust's normal Windows argument builder emits. The command text is
        // assembled exclusively from fixed system paths and a validated
        // generated helper path, so pass this one argument verbatim.
        .raw_arg(format!("/D /S /C \"{command_line}\""))
        .creation_flags(0x08000000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            anyhow::anyhow!(
                "Failed to start Windows cleanup of COPE helper {}: {error}",
                canonical_path.display()
            )
        })
}

pub fn run_deferred_cleanup(parent_pid: u32) -> Result<()> {
    let dir = verified_cope_dir()?;
    wait_for_parent_exit(parent_pid)?;

    remove_owned_data_files(&dir)?;
    let installed_exe = dir.join("cope.exe");
    if installed_exe.exists() {
        fs::remove_file(&installed_exe).map_err(|error| {
            anyhow::anyhow!(
                "Failed to remove installed COPE executable {}: {error}",
                installed_exe.display()
            )
        })?;
    }
    let _ = remove_cope_dir_if_empty(&dir)?;

    // The helper is itself a COPE executable. A detached Windows cleanup
    // process removes it after this process exits.
    schedule_helper_self_delete(&current_exe_path()?)?;
    Ok(())
}

pub fn remove_from_user_path(dir: &std::path::Path) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let paths_key =
        hkcu.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE | KEY_CREATE_SUB_KEY)?;

    let current_path = paths_key
        .get_value::<String, _>("PATH")
        .unwrap_or_else(|_| String::new());

    let dir_str = dir.to_string_lossy().to_string();
    let dir_str_lower = dir_str.to_lowercase();

    let parts: Vec<&str> = current_path
        .split(';')
        .filter(|part| part.trim().to_lowercase() != dir_str_lower)
        .collect();
    if parts.len() != current_path.split(';').count() {
        let new_path = parts.join(";");
        if new_path != current_path {
            paths_key.set_value("PATH", &new_path)?;
        }
    }

    Ok(())
}

/// Acquire a global mutex to ensure only one COPE daemon runs system-wide.
/// Returns the mutex handle if acquired, None if already held by another process.
pub fn acquire_global_mutex() -> Result<Option<HANDLE>> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;

    let mutex_name: Vec<u16> = global_mutex_name()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let handle = unsafe { CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr())) };

    match handle {
        Ok(h) if !h.is_invalid() => {
            let err = unsafe { GetLastError() };
            if err == ERROR_ALREADY_EXISTS {
                // Mutex already exists - another instance is running
                let _ = unsafe { CloseHandle(h) };
                Ok(None)
            } else {
                Ok(Some(h))
            }
        }
        _ => Ok(None),
    }
}

/// Release the global mutex.
pub fn release_global_mutex(handle: HANDLE) {
    let _ = unsafe { CloseHandle(handle) };
}

pub fn ensure_single_instance() -> Result<Option<HANDLE>> {
    acquire_global_mutex()
}

/// Start the COPE daemon in the background.
///
/// Idempotent lifecycle semantics:
/// - `Ok(true)`  — a new daemon was started.
/// - `Ok(false)` — a daemon was already running; nothing was spawned.
///   This is NOT an error.
/// - `Err`       — genuine startup failure (spawn error or startup timeout).
pub fn start_background(exe_path: Option<PathBuf>) -> Result<bool> {
    if is_daemon_running()? {
        return Ok(false);
    }

    let path = match exe_path {
        Some(path) => path,
        None => installed_exe_path()?,
    };

    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let child = Command::new(&path)
        .arg("daemon")
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    drop(child);

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if Instant::now() >= deadline {
            anyhow::bail!("COPE daemon did not confirm startup within timeout.");
        }

        if is_daemon_running()? {
            return Ok(true);
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn run_daemon(config: crate::config::Config) -> Result<()> {
    // Deterministic lifecycle tests use a sleeping daemon so they never depend
    // on desktop-global RegisterHotKey ownership. This seam is inert unless
    // the explicit test value is present and is not part of normal operation.
    if test_daemon_mode() {
        write_daemon_pid()?;
        std::thread::park();
        return Ok(());
    }

    let config = Arc::new(std::sync::RwLock::new(config));
    let mut manager = HotkeyManager::new(config.clone())?;
    manager.register_hotkeys()?;
    if !manager.get_failed_registrations().is_empty() {
        anyhow::bail!("One or more required global hotkeys could not be registered");
    }

    write_daemon_pid()?;
    let run_result = manager.run();
    let cleanup_result = cleanup_daemon_pid();
    run_result?;
    cleanup_result?;
    Ok(())
}

fn write_daemon_pid() -> Result<()> {
    let pid_path = daemon_pid_path()?;
    let pid = std::process::id();
    std::fs::write(&pid_path, pid.to_string())?;
    Ok(())
}

fn daemon_pid_path() -> Result<PathBuf> {
    let config_dir = crate::config::config_dir()?;
    Ok(config_dir.join("daemon.pid"))
}

fn cleanup_daemon_pid() -> Result<()> {
    let pid_path = daemon_pid_path()?;
    if pid_path.exists() {
        let _ = std::fs::remove_file(&pid_path);
    }
    Ok(())
}

fn read_daemon_pid() -> Result<Option<u32>> {
    let pid_path = daemon_pid_path()?;
    if !pid_path.exists() {
        return Ok(None);
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            let _ = std::fs::remove_file(&pid_path);
            return Ok(None);
        }
    };

    if pid == 0 {
        let _ = std::fs::remove_file(&pid_path);
        return Ok(None);
    }

    Ok(Some(pid))
}

/// Verify that the given process handle points to the installed COPE daemon
/// by comparing its executable path against the expected install location.
///
/// Uses the SAME handle for both query and comparison — the caller must
/// ensure this handle was opened with PROCESS_QUERY_LIMITED_INFORMATION.
fn verify_handle_is_cope_daemon(handle: windows::Win32::Foundation::HANDLE) -> bool {
    use windows::Win32::System::Threading::{QueryFullProcessImageNameW, PROCESS_NAME_FORMAT};

    let installed_path = match installed_exe_path() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let installed_str = installed_path.to_string_lossy().to_lowercase();

    unsafe {
        let mut buf = vec![0u16; 520];
        let mut size = buf.len() as u32;

        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        );

        if result.is_ok() && size > 0 {
            let path_str = String::from_utf16_lossy(&buf[..size as usize]);
            path_str.to_lowercase() == installed_str
        } else {
            false
        }
    }
}

fn is_handle_alive(handle: windows::Win32::Foundation::HANDLE) -> bool {
    use windows::Win32::System::Threading::GetExitCodeProcess;

    unsafe {
        let mut exit_code = 0u32;
        let result = GetExitCodeProcess(handle, &mut exit_code);
        result.is_ok() && exit_code == 259 // STILL_ACTIVE
    }
}

pub fn is_daemon_running() -> Result<bool> {
    let pid = match read_daemon_pid()? {
        Some(p) => p,
        None => return Ok(false),
    };

    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
    let handle = match handle {
        Ok(h) if !h.is_invalid() => h,
        _ => {
            let _ = std::fs::remove_file(daemon_pid_path()?);
            return Ok(false);
        }
    };

    let alive = is_handle_alive(handle);
    let is_cope = verify_handle_is_cope_daemon(handle);
    let _ = unsafe { CloseHandle(handle) };

    if !alive || !is_cope {
        let _ = std::fs::remove_file(daemon_pid_path()?);
        return Ok(false);
    }

    Ok(true)
}

pub fn stop_daemon() -> Result<bool> {
    let pid = match read_daemon_pid()? {
        Some(p) => p,
        None => return Ok(false),
    };

    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    // Open with both query and terminate permissions so we can verify identity
    // and terminate using the SAME handle — eliminating the TOCTOU window.
    let handle = unsafe {
        OpenProcess(
            windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION
                | PROCESS_TERMINATE,
            false,
            pid,
        )
    };
    let handle = match handle {
        Ok(h) if !h.is_invalid() => h,
        _ => {
            // Cannot open process — PID is stale/dead. Safe to remove.
            let _ = std::fs::remove_file(daemon_pid_path()?);
            return Ok(false);
        }
    };

    // Verify identity using the SAME handle we will terminate
    if !verify_handle_is_cope_daemon(handle) {
        let _ = unsafe { CloseHandle(handle) };
        // PID points to an unrelated process — stale. Safe to remove.
        let _ = std::fs::remove_file(daemon_pid_path()?);
        return Ok(false);
    }

    // Terminate using the SAME handle we just verified.
    // PID file is NOT removed until we confirm the process actually exited.
    let result = unsafe { TerminateProcess(handle, 0) };
    let _ = unsafe { CloseHandle(handle) };

    if result.is_err() {
        // TerminateProcess failed. Daemon is still running. Preserve PID file
        // so `cope status` can still identify the daemon.
        anyhow::bail!(
            "Failed to terminate COPE daemon process {}. The process may lack permissions.",
            pid
        );
    }

    // Wait briefly for graceful shutdown, then verify exit.
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Post-termination check: open a fresh handle just to confirm the process is gone.
    if let Some(verify_handle) = open_process_for_query(pid) {
        let still_alive = is_handle_alive(verify_handle);
        let _ = unsafe { CloseHandle(verify_handle) };
        if still_alive {
            // Process did not exit. Preserve PID file so status still works.
            anyhow::bail!(
                "COPE daemon process {} did not exit after termination.",
                pid
            );
        }
    }

    // Process confirmed dead. Now safe to remove PID file.
    let _ = std::fs::remove_file(daemon_pid_path()?);
    Ok(true)
}

fn open_process_for_query(pid: u32) -> Option<windows::Win32::Foundation::HANDLE> {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        if handle.is_invalid() {
            return None;
        }
        Some(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_exe_path() {
        let path = current_exe_path().unwrap();
        assert!(path.exists());
        assert!(path.file_name().unwrap().to_string_lossy().contains("cope"));
    }

    #[test]
    fn test_installed_exe_path() {
        let path = installed_exe_path().unwrap();
        assert_eq!(path.file_name().unwrap().to_string_lossy(), "cope.exe");
    }

    #[test]
    fn test_startup_command() {
        let path = std::path::Path::new(r"C:\Users\foo\AppData\Local\COPE\cope.exe");
        assert_eq!(
            startup_command(path),
            r#""C:\Users\foo\AppData\Local\COPE\cope.exe" daemon"#
        );
    }

    #[test]
    fn test_startup_command_spaces() {
        let path = std::path::Path::new(r"C:\Users\John Doe\AppData\Local\COPE\cope.exe");
        assert_eq!(
            startup_command(path),
            r#""C:\Users\John Doe\AppData\Local\COPE\cope.exe" daemon"#
        );
    }

    #[test]
    fn test_cleanup_helper_launcher_removes_validated_temp_file() {
        let path = std::env::temp_dir().join(format!(
            "cope-uninstall-test-{}-{}.exe",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"test").unwrap();
        schedule_helper_self_delete(&path).unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while path.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = std::fs::remove_file(&path);
        assert!(
            !path.exists(),
            "cleanup launcher did not remove helper file"
        );
    }

    // --- Daemon stop PID file lifecycle tests ---

    use crate::config::config_dir;

    fn write_fake_pid_file(pid: u32) -> std::path::PathBuf {
        let pid_path = config_dir().unwrap().join("daemon.pid");
        std::fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
        std::fs::write(&pid_path, pid.to_string()).unwrap();
        pid_path
    }

    #[test]
    fn test_stop_daemon_no_pid_file_returns_false() {
        let pid_path = config_dir().unwrap().join("daemon.pid");
        let _ = std::fs::remove_file(&pid_path);
        let result = stop_daemon().unwrap();
        assert!(!result);
    }

    #[test]
    fn test_stop_daemon_stale_pid_removes_pid_file() {
        // PID 1 exists (System Idle Process) but is not a COPE daemon.
        // The stale-PID branch (verify_handle_is_cope_daemon returns false)
        // should remove the PID file and return Ok(false).
        let pid_path = config_dir().unwrap().join("daemon.pid");
        write_fake_pid_file(1);
        let result = stop_daemon().unwrap();
        assert!(!result);
        assert!(
            !pid_path.exists(),
            "PID file should be removed for stale PID"
        );
    }

    #[test]
    fn test_stop_daemon_nonexistent_pid_removes_pid_file() {
        // PID 99999 does not exist. OpenProcess fails -> stale -> remove.
        let pid_path = config_dir().unwrap().join("daemon.pid");
        write_fake_pid_file(99999);
        let result = stop_daemon().unwrap();
        assert!(!result);
        assert!(
            !pid_path.exists(),
            "PID file should be removed when PID does not exist"
        );
    }
}
