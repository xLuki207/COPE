use crate::config::Config;
use crate::history::History;
use crate::parser::{extract_solana_addresses, ExtractResult, SolanaAddress};
use crate::routes::{open_destination, Destination};
use anyhow::Result;
use arboard::Clipboard;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage,
    RegisterClassExW, MSG, WM_HOTKEY, WM_QUIT, WNDCLASSEXW, WS_OVERLAPPED,
};

extern "system" {
    fn GetClipboardSequenceNumber() -> u32;
}

const HOTKEY_ID_BASE: i32 = 1000;

pub struct HotkeyManager {
    hwnd: HWND,
    registered_hotkeys: HashMap<i32, Destination>,
    running: Arc<AtomicBool>,
    config: Arc<std::sync::RwLock<Config>>,
    failed_registrations: Vec<String>,
    current_ca: Option<SolanaAddress>,
    last_clipboard_text: Option<String>,
    last_clipboard_sequence: Option<u32>,
    pending_clipboard_restore: Option<String>,
    history: History,
}

impl HotkeyManager {
    pub fn new(config: Arc<std::sync::RwLock<Config>>) -> Result<Self> {
        let running = Arc::new(AtomicBool::new(false));
        let hwnd = create_message_window()?;
        let history = History::new()?;
        Ok(Self {
            hwnd,
            registered_hotkeys: HashMap::new(),
            running,
            config,
            failed_registrations: Vec::new(),
            current_ca: None,
            last_clipboard_text: None,
            last_clipboard_sequence: None,
            pending_clipboard_restore: None,
            history,
        })
    }

    pub fn register_hotkeys(&mut self) -> Result<()> {
        self.unregister_all()?;
        self.failed_registrations.clear();

        let config = self.config.read().unwrap();
        let mut id = HOTKEY_ID_BASE;

        for (dest, route) in config.enabled_routes() {
            if route.enabled {
                let mods = HOT_KEY_MODIFIERS(route.modifiers);
                let vk = route.vk_code;

                unsafe {
                    let result = RegisterHotKey(self.hwnd, id, mods, vk);
                    if result.is_ok() {
                        self.registered_hotkeys.insert(id, *dest);
                        info!("Registered hotkey {} for {:?}", route.hotkey_string(), dest);
                    } else {
                        let err_msg = format!(
                            "Failed to register hotkey {} for {}: already in use by another application",
                            route.hotkey_string(),
                            dest.display_name()
                        );
                        self.failed_registrations.push(err_msg.clone());
                        warn!("{}", err_msg);
                    }
                }
                id += 1;
            }
        }

        Ok(())
    }

    pub fn get_failed_registrations(&self) -> &[String] {
        &self.failed_registrations
    }

    pub fn unregister_all(&mut self) -> Result<()> {
        for id in self.registered_hotkeys.keys() {
            unsafe {
                let _ = UnregisterHotKey(self.hwnd, *id);
            }
        }
        self.registered_hotkeys.clear();
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        info!("Hotkey manager started");

        let mut msg = MSG::default();
        while self.running.load(Ordering::SeqCst) {
            let result = unsafe { GetMessageW(&mut msg, HWND(std::ptr::null_mut()), 0, 0) };
            if result.into() {
                if msg.message == WM_HOTKEY {
                    let hotkey_id = msg.wParam.0 as i32;
                    if let Some(dest) = self.registered_hotkeys.get(&hotkey_id) {
                        self.handle_hotkey(*dest);
                    }
                } else if msg.message == WM_QUIT {
                    break;
                }
                unsafe {
                    DispatchMessageW(&msg);
                }
            } else {
                break;
            }
        }

        info!("Hotkey manager stopped");
        Ok(())
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        unsafe {
            PostQuitMessage(0);
        }
    }

    fn handle_hotkey(&mut self, destination: Destination) {
        debug!("Hotkey triggered for {:?}", destination);

        let (resolved_ca, should_route, feedback) = self.resolve_current_ca(destination);

        if should_route {
            if let Some(addr) = resolved_ca {
                match open_destination(destination, &addr) {
                    Ok(()) => self.history.record(addr.as_str(), destination),
                    Err(e) => {
                        error!("Failed to open destination: {}", e);
                        eprintln!("Failed to open destination: {}", e);
                    }
                }
            }
        }

        if let Some(before) = self.pending_clipboard_restore.take() {
            let _ = self.write_clipboard(&before.clone());
            // Update last_clipboard_text so the next hotkey invocation does NOT
            // mistake our own restore for a manual clipboard change.
            self.last_clipboard_text = Some(before);
            self.last_clipboard_sequence = Some(unsafe { GetClipboardSequenceNumber() });
        }

        if let Some(f) = feedback {
            eprintln!("{}", f);
        }
    }

    /// Extract a single Solana address from text, or return feedback on failure.
    fn try_extract_address(text: &str) -> (Option<SolanaAddress>, Option<String>) {
        match extract_solana_addresses(text) {
            Ok(ExtractResult::Single(addr)) => (Some(addr), None),
            Ok(ExtractResult::Multiple(_)) => {
                (None, Some("Multiple CAs found. Highlight one.".to_string()))
            }
            Ok(ExtractResult::None) => (None, Some("No Solana CA found.".to_string())),
            Err(e) => {
                error!("Failed to extract addresses: {}", e);
                (None, Some(format!("Error: {e}")))
            }
        }
    }

    /// Resolve which CA to route for this hotkey press.
    ///
    /// Invariants:
    ///   0 valid fresh CAs -> do not overwrite current_ca, return (None, false, feedback)
    ///   1 valid fresh CA  -> update current_ca, return (Some(ca), true, None)
    ///   2+ valid fresh CAs -> do not guess, return (None, false, feedback)
    ///   no fresh selection -> reuse current_ca if present
    ///
    /// On success path (should_route=true), sets self.pending_clipboard_restore
    /// so the caller can restore clipboard AFTER browser dispatch.
    fn resolve_current_ca(
        &mut self,
        _destination: Destination,
    ) -> (Option<SolanaAddress>, bool, Option<String>) {
        let (selected_text, _user_selection, text_before, clipboard_sequence_before) =
            self.try_get_selected_ca();

        if let Some(text) = selected_text {
            let (addr, feedback) = Self::try_extract_address(&text);
            if let Some(addr) = addr {
                self.current_ca = Some(addr.clone());
                self.last_clipboard_text = Some(text);
                self.pending_clipboard_restore = text_before;
                (Some(addr), true, None)
            } else {
                if let Some(before) = text_before {
                    let _ = self.write_clipboard(&before);
                    self.last_clipboard_text = Some(before);
                    self.last_clipboard_sequence = Some(unsafe { GetClipboardSequenceNumber() });
                }
                (None, false, feedback)
            }
        } else {
            // `text_before` is the clipboard state before COPE synthesized
            // Ctrl+C. It is the reliable signal for a manually copied coin.
            // Do not use a value captured after Ctrl+C: that value can make a
            // newly copied coin look identical to the previous sticky coin.
            let current_clipboard_text = match text_before {
                Some(text) => text,
                None => match self.read_clipboard() {
                    Ok(t) => t,
                    Err(_) => {
                        return (None, false, Some("No Solana CA found.".to_string()));
                    }
                },
            };
            let clipboard_changed = clipboard_changed_since(
                self.last_clipboard_text.as_deref(),
                &current_clipboard_text,
                self.last_clipboard_sequence,
                clipboard_sequence_before,
            );

            // A changed clipboard is a new user-copied coin and must replace
            // the sticky selected coin. Also initialize from a valid clipboard
            // coin on the first hotkey press.
            if clipboard_changed || self.current_ca.is_none() {
                let (addr, feedback) = Self::try_extract_address(&current_clipboard_text);
                if let Some(addr) = addr {
                    self.current_ca = Some(addr.clone());
                    self.last_clipboard_text = Some(current_clipboard_text);
                    self.last_clipboard_sequence = Some(unsafe { GetClipboardSequenceNumber() });
                    (Some(addr), true, None)
                } else {
                    (None, false, feedback)
                }
            } else if let Some(ref addr) = self.current_ca {
                // No selection and no new clipboard change: reuse the most
                // recently selected or copied coin.
                self.last_clipboard_text = Some(current_clipboard_text);
                self.last_clipboard_sequence = Some(unsafe { GetClipboardSequenceNumber() });
                (Some(addr.clone()), true, None)
            } else {
                (None, false, Some("No Solana CA found.".to_string()))
            }
        }
    }

    /// Capture selected text via Ctrl+C synthesis.
    ///
    /// Snapshot clipboard, wait for Alt release, inject Ctrl+C, then read the
    /// new clipboard content. Returns (selected_text, user_selection,
    /// text_before, clipboard_sequence_before).
    /// The caller is responsible for clipboard restoration after dispatch.
    fn try_get_selected_ca(&mut self) -> (Option<String>, bool, Option<String>, u32) {
        let text_before = self.read_clipboard().ok();
        let seq_before = unsafe { GetClipboardSequenceNumber() };

        // Alt must be released before injecting Ctrl+C or Windows may interpret
        // the synthetic copy as part of the COPE hotkey.
        self.wait_for_alt_release();

        self.send_ctrl_c();

        let text_after = self.wait_for_clipboard_change(seq_before, 150);

        let user_selection = match (&text_before, &text_after) {
            (Some(b), Some(a)) => b != a && !a.trim().is_empty(),
            _ => false,
        };

        let selected_text = if user_selection {
            text_after.clone()
        } else {
            None
        };

        (selected_text, user_selection, text_before, seq_before)
    }

    /// Wait for Alt key release using actual Windows key state.
    /// Fast spin for ~200μs, then bounded 1ms poll up to 500ms max.
    fn wait_for_alt_release(&self) {
        let vk_alt: i32 = 0x12;
        let pressed: i16 = 0x8000u16 as i16;

        let t0 = Instant::now();
        while t0.elapsed().as_micros() < 200 {
            if unsafe { GetAsyncKeyState(vk_alt) & pressed } == 0 {
                return;
            }
            std::hint::spin_loop();
        }

        let mut waited = 0u32;
        while waited < 500 {
            if unsafe { GetAsyncKeyState(vk_alt) & pressed } == 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
            waited += 1;
        }
    }

    /// Wait for clipboard sequence number to change after Ctrl+C injection.
    fn wait_for_clipboard_change(&self, seq_before: u32, total_ms: u64) -> Option<String> {
        let deadline = Instant::now() + Duration::from_millis(total_ms);

        let t0 = Instant::now();
        while t0.elapsed().as_micros() < 500 {
            if unsafe { GetClipboardSequenceNumber() } != seq_before {
                return self.read_clipboard().ok();
            }
            std::hint::spin_loop();
        }

        let delays: &[u64] = &[1, 2, 5, 10, 10, 10, 10, 10];
        for &d in delays {
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(d));
            if unsafe { GetClipboardSequenceNumber() } != seq_before {
                return self.read_clipboard().ok();
            }
        }

        if Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            if unsafe { GetClipboardSequenceNumber() } != seq_before {
                return self.read_clipboard().ok();
            }
        }

        None
    }

    fn read_clipboard(&self) -> Result<String> {
        let mut clipboard = Clipboard::new()?;
        let text = clipboard.get_text()?;
        Ok(text)
    }

    fn write_clipboard(&self, text: &str) -> Result<()> {
        let mut clipboard = Clipboard::new()?;
        clipboard.set_text(text)?;
        Ok(())
    }

    fn send_ctrl_c(&self) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_C, VK_CONTROL};

        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: Default::default(),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_C,
                        wScan: 0,
                        dwFlags: Default::default(),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_C,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

fn clipboard_changed_since(
    last_clipboard_text: Option<&str>,
    current_clipboard_text: &str,
    last_clipboard_sequence: Option<u32>,
    current_clipboard_sequence: u32,
) -> bool {
    last_clipboard_text != Some(current_clipboard_text)
        || last_clipboard_sequence != Some(current_clipboard_sequence)
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        let _ = self.unregister_all();
    }
}

fn create_message_window() -> Result<HWND> {
    use windows::core::PCWSTR;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;

    let class_name = "COPE_HOTKEY_WINDOW\0".encode_utf16().collect::<Vec<u16>>();
    let hinstance = unsafe { GetModuleHandleW(None)? };

    let wnd_class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        hInstance: hinstance.into(),
        lpfnWndProc: Some(window_proc),
        style: Default::default(),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hIcon: Default::default(),
        hCursor: Default::default(),
        hbrBackground: Default::default(),
        lpszMenuName: PCWSTR::null(),
        hIconSm: Default::default(),
    };

    unsafe {
        let atom = RegisterClassExW(&wnd_class);
        if atom == 0 {
            return Err(anyhow::anyhow!("Failed to register window class"));
        }

        let hwnd = CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        )?;

        Ok(hwnd)
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CA: &str = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
    const VALID_CA_2: &str = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";

    #[test]
    fn test_extract_single_valid_ca() {
        let (addr, feedback) = HotkeyManager::try_extract_address(VALID_CA);
        assert_eq!(addr.unwrap().as_str(), VALID_CA);
        assert!(feedback.is_none());
    }

    #[test]
    fn test_extract_no_ca_returns_none() {
        let (addr, feedback) = HotkeyManager::try_extract_address("hello world no CA here");
        assert!(addr.is_none());
        assert!(feedback.is_some());
    }

    #[test]
    fn test_extract_two_cas_returns_none() {
        let text = format!("{} {}", VALID_CA, VALID_CA_2);
        let (addr, feedback) = HotkeyManager::try_extract_address(&text);
        assert!(addr.is_none());
        assert!(feedback.unwrap().contains("Multiple"));
    }

    #[test]
    fn test_sticky_ca_not_overridden_by_zero_valid() {
        // Simulate the sticky CA invariant: when resolve_current_ca detects
        // 0 valid fresh CAs, it must NOT overwrite self.current_ca.
        // We test the try_extract_address path directly — it returns None
        // for 0 CAs, which is the trigger for "do not overwrite".
        let (addr, _) = HotkeyManager::try_extract_address("not a CA");
        assert!(addr.is_none(), "Zero CAs must not overwrite sticky CA");
    }

    #[test]
    fn test_sticky_ca_not_overridden_by_two_valid() {
        // Simulate the sticky CA invariant: when resolve_current_ca detects
        // 2+ valid CAs, it must NOT guess — returns None.
        let text = format!("{} {}", VALID_CA, VALID_CA_2);
        let (addr, _) = HotkeyManager::try_extract_address(&text);
        assert!(addr.is_none(), "Multiple CAs must not overwrite sticky CA");
    }

    #[test]
    fn test_newly_copied_clipboard_replaces_sticky_snapshot() {
        assert!(clipboard_changed_since(
            Some(VALID_CA),
            VALID_CA_2,
            Some(10),
            10
        ));
    }

    #[test]
    fn test_unchanged_clipboard_reuses_sticky_snapshot() {
        assert!(!clipboard_changed_since(
            Some(VALID_CA),
            VALID_CA,
            Some(10),
            10
        ));
        assert!(clipboard_changed_since(
            Some(VALID_CA),
            VALID_CA,
            Some(10),
            11
        ));
        assert!(clipboard_changed_since(None, VALID_CA, None, 10));
    }
}
