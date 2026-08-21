use crate::hotkeys::HotkeyManager;
use anyhow::Result;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use winreg::enums::*;
use winreg::RegKey;

const APP_NAME: &str = "COPE";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

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
    let run_key = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
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
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\User\\AppData\\Local"));
    let path = local_app_data.join("COPE").join("cope.exe");
    Ok(path)
}

pub fn installed_cope_dir() -> Result<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\User\\AppData\\Local"));
    Ok(local_app_data.join("COPE"))
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

    if current_path.to_lowercase().contains(&dir_str_lower) {
        let mut new_path = String::new();
        for part in current_path.split(';') {
            let part_trimmed = part.trim();
            if part_trimmed.to_lowercase() != dir_str_lower {
                if !new_path.is_empty() {
                    new_path.push(';');
                }
                new_path.push_str(part_trimmed);
            }
        }
        if new_path != current_path {
            paths_key.set_value("PATH", &new_path)?;
        }
    }

    Ok(())
}

pub fn start_background(exe_path: Option<PathBuf>) -> Result<()> {
    let path = exe_path
        .unwrap_or_else(|| installed_exe_path().expect("failed to determine installed exe path"));

    use std::process::Stdio;
    let child = Command::new(&path)
        .arg("daemon")
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let pid = child.id();
    let pid_path = daemon_pid_path()?;
    std::fs::write(&pid_path, pid.to_string())?;
    Ok(())
}

pub fn run_daemon(config: crate::config::Config) -> Result<()> {
    write_daemon_pid()?;

    let config = Arc::new(std::sync::RwLock::new(config));
    let mut manager = HotkeyManager::new(config.clone())?;
    manager.register_hotkeys()?;
    manager.run()?;

    cleanup_daemon_pid()?;
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

pub fn ensure_single_instance() -> Result<single_instance::SingleInstance> {
    let instance =
        single_instance::SingleInstance::new("COPE").map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(instance)
}

pub fn stop_daemon() -> Result<bool> {
    let pid_path = daemon_pid_path()?;
    if !pid_path.exists() {
        return Ok(false);
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str.trim().parse().unwrap_or(0);

    if pid == 0 {
        let _ = std::fs::remove_file(&pid_path);
        return Ok(false);
    }

    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid)?;
        if !handle.is_invalid() {
            TerminateProcess(handle, 0)?;
            let _ = std::fs::remove_file(&pid_path);
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn is_daemon_running() -> Result<bool> {
    let pid_path = daemon_pid_path()?;
    if !pid_path.exists() {
        return Ok(false);
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str.trim().parse().unwrap_or(0);

    if pid == 0 {
        let _ = std::fs::remove_file(&pid_path);
        return Ok(false);
    }

    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        match handle {
            Ok(h) => {
                if !h.is_invalid() {
                    let mut exit_code = 0u32;
                    use windows::Win32::System::Threading::GetExitCodeProcess;
                    let _ = GetExitCodeProcess(h, &mut exit_code);
                    use windows::Win32::Foundation::CloseHandle;
                    let _ = CloseHandle(h);
                    return Ok(exit_code == 259); // STILL_ACTIVE
                }
            }
            Err(_) => {
                let _ = std::fs::remove_file(&pid_path);
            }
        }
    }
    Ok(false)
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
}
