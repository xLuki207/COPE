use crate::config::Config;
use crate::parser::{extract_solana_addresses, ExtractResult, SolanaAddress};
use crate::routes::{open_destination, Destination};
use anyhow::Result;
use arboard::Clipboard;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
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

const HOTKEY_ID_BASE: i32 = 1000;

pub struct HotkeyManager {
    hwnd: HWND,
    registered_hotkeys: HashMap<i32, Destination>,
    running: Arc<AtomicBool>,
    config: Arc<std::sync::RwLock<Config>>,
    failed_registrations: Vec<String>,
    current_ca: Option<SolanaAddress>,
    last_clipboard_text: Option<String>,
}

impl HotkeyManager {
    pub fn new(config: Arc<std::sync::RwLock<Config>>) -> Result<Self> {
        let running = Arc::new(AtomicBool::new(false));
        let hwnd = create_message_window()?;
        Ok(Self {
            hwnd,
            registered_hotkeys: HashMap::new(),
            running,
            config,
            failed_registrations: Vec::new(),
            current_ca: None,
            last_clipboard_text: None,
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

    #[allow(dead_code)]
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

        let (new_ca, should_route, feedback) = self.resolve_current_ca(destination);

        if should_route {
            if let Some(addr) = &new_ca {
                info!(
                    "Found single CA: {}, opening {:?}",
                    addr.as_str(),
                    destination
                );
                if let Err(e) = open_destination(destination, addr) {
                    error!("Failed to open destination: {}", e);
                    eprintln!("Failed to open destination: {}", e);
                }
            } else if let Some(ref addr) = self.current_ca {
                info!(
                    "Reusing current CA: {}, opening {:?}",
                    addr.as_str(),
                    destination
                );
                if let Err(e) = open_destination(destination, addr) {
                    error!("Failed to open destination: {}", e);
                    eprintln!("Failed to open destination: {}", e);
                }
            }
        }

        if let Some(f) = feedback {
            eprintln!("{}", f);
        }
    }

    fn resolve_current_ca(
        &mut self,
        _destination: Destination,
    ) -> (Option<SolanaAddress>, bool, Option<String>) {
        // Step A: Detect current selected text via Ctrl+C
        let (_selected_text, _user_selection) = self.try_get_selected_ca();

        if let Some(text) = _selected_text {
            // Case A: selection successfully captured
            match extract_solana_addresses(&text) {
                Ok(ExtractResult::Single(single_addr)) => {
                    // Selection contains exactly one valid Solana CA
                    let mut new_ca = self.clone_for_ca_update();
                    new_ca.current_ca = Some(single_addr.clone());
                    new_ca.last_clipboard_text = Some(text.clone());
                    drop(new_ca);
                    (Some(single_addr), true, None)
                }
                Ok(ExtractResult::Multiple(_addrs)) => {
                    // Selection contains multiple distinct CAs - do nothing, do not guess
                    let _ = self.restore_user_clipboard();
                    (
                        None,
                        false,
                        Some("Multiple CAs found. Highlight one.".to_string()),
                    )
                }
                Ok(ExtractResult::None) => {
                    // Selection contains no valid CA - do nothing, DO NOT fall back to stale current_ca
                    let _ = self.restore_user_clipboard();
                    (None, false, Some("No Solana CA found.".to_string()))
                }
                Err(e) => {
                    error!("Failed to extract addresses: {}", e);
                    let _ = self.restore_user_clipboard();
                    (None, false, Some(format!("Error: {}", e)))
                }
            }
        } else {
            // No user selection captured (seq did not change after Ctrl+C)
            // Case B or C: check if user manually changed clipboard

            let current_clipboard_text = match self.read_clipboard() {
                Ok(t) => t,
                Err(_) => {
                    return (None, false, Some("No Solana CA found.".to_string()));
                }
            };

            if let Some(last_text) = &self.last_clipboard_text {
                if current_clipboard_text != *last_text {
                    // The user has manually changed the clipboard since COPE last handled it
                    // Case B: user clipboard changed
                    match extract_solana_addresses(&current_clipboard_text) {
                        Ok(ExtractResult::Single(addr)) => {
                            // Exactly one CA in the user's manually changed clipboard
                            let mut cm = self.clone_for_ca_update();
                            cm.current_ca = Some(addr.clone());
                            cm.last_clipboard_text = Some(current_clipboard_text.clone());
                            drop(cm);
                            // Update last_clipboard_text to current text
                            self.last_clipboard_text = Some(current_clipboard_text.clone());
                            (Some(addr), true, None)
                        }
                        Ok(ExtractResult::Multiple(_addrs)) => {
                            // Multiple CAs - do nothing, do not guess
                            (None, false, Some("Multiple CAs found.".to_string()))
                        }
                        Ok(ExtractResult::None) => {
                            // Invalid text - do nothing, do not reuse stale current_ca
                            (None, false, Some("No Solana CA found.".to_string()))
                        }
                        Err(e) => {
                            error!("Failed to extract addresses: {}", e);
                            (None, false, Some(format!("Error: {}", e)))
                        }
                    }
                } else {
                    // Case C: no selection and no new user clipboard change
                    if self.current_ca.is_some() {
                        // Reuse existing current_ca
                        let addr = self.current_ca.clone().unwrap();
                        (Some(addr), true, None)
                    } else {
                        // No current_ca and no new selection
                        // First run with no prior state: check clipboard for CA
                        // (handles the "manual copy must work" case when called first time)
                        match extract_solana_addresses(&current_clipboard_text) {
                            Ok(ExtractResult::Single(addr)) => {
                                // Set as current_ca (the "manual copy must work" case)
                                let mut cm = self.clone_for_ca_update();
                                cm.current_ca = Some(addr.clone());
                                cm.last_clipboard_text = Some(current_clipboard_text.clone());
                                drop(cm);
                                (Some(addr), true, None)
                            }
                            Ok(ExtractResult::Multiple(_addrs)) => {
                                (None, false, Some("Multiple CAs found.".to_string()))
                            }
                            Ok(ExtractResult::None) => {
                                (None, false, Some("No Solana CA found.".to_string()))
                            }
                            Err(e) => {
                                error!("Failed to extract addresses: {}", e);
                                (None, false, Some(format!("Error: {}", e)))
                            }
                        }
                    }
                }
            } else {
                // first run: last_clipboard_text is None
                // This handles the "manual copy must work" case:
                // if clipboard already contains a valid CA with no prior COPE state,
                // use it directly.
                match extract_solana_addresses(&current_clipboard_text) {
                    Ok(ExtractResult::Single(addr)) => {
                        // Set as current_ca (the "manual copy must work" case)
                        let mut cm = self.clone_for_ca_update();
                        cm.current_ca = Some(addr.clone());
                        cm.last_clipboard_text = Some(current_clipboard_text.clone());
                        drop(cm);
                        (Some(addr), true, None)
                    }
                    Ok(ExtractResult::Multiple(_addrs)) => {
                        (None, false, Some("Multiple CAs found.".to_string()))
                    }
                    Ok(ExtractResult::None) => {
                        (None, false, Some("No Solana CA found.".to_string()))
                    }
                    Err(e) => {
                        error!("Failed to extract addresses: {}", e);
                        (None, false, Some(format!("Error: {}", e)))
                    }
                }
            }
        }
    }

    fn clone_for_ca_update(&self) -> HotkeyManagerClone {
        HotkeyManagerClone {
            current_ca: self.current_ca.clone(),
            last_clipboard_text: self.last_clipboard_text.clone(),
        }
    }

    fn try_get_selected_ca(&mut self) -> (Option<String>, bool) {
        // Snapshot current clipboard text before Ctrl+C
        let text_before = self.read_clipboard().ok();

        // Send Ctrl+C to capture selection
        // Wait for modifier keys (Alt) to be released before synthesizing Ctrl+C.
        // This prevents the physical Alt key from interfering with Ctrl+C synthesis.
        let mut waited = 0;
        while unsafe { GetAsyncKeyState(0x12) & (0x8000u16 as i16) != 0 } && waited < 500 {
            std::thread::sleep(Duration::from_millis(10));
            waited += 10;
        }

        self.send_ctrl_c();
        std::thread::sleep(Duration::from_millis(50));

        // Read clipboard after Ctrl+C
        let text_after = self.read_clipboard().ok();

        // Restore the user's clipboard from before-Ctrl+C snapshot
        // This ensures COPE's Ctrl+C doesn't permanently change user clipboard
        if let Some(ref before) = text_before {
            let _ = self.write_clipboard(before);
        }

        // Determine if user made a new selection.
        // User selection: clipboard text before was different from after,
        // AND the after-text is not empty (we successfully captured something)
        let user_selection = {
            let before = text_before.as_deref();
            let after = text_after.as_deref();
            match (before, after) {
                (Some(b), Some(a)) => b != a && !a.trim().is_empty(),
                _ => false,
            }
        };

        let selected_text = if user_selection {
            text_after.clone()
        } else {
            None
        };

        // Update last_clipboard_text after COPE's operation.
        // If user made a selection, record the selected text.
        // If no user selection, record the current clipboard text (after restoration)
        // so we can detect manual changes on the next hotkey.
        let final_clipboard_text: Option<String> = if user_selection {
            text_after.clone()
        } else {
            self.read_clipboard().ok()
        };

        self.last_clipboard_text = final_clipboard_text;

        (selected_text, user_selection)
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

    fn restore_user_clipboard(&self) -> Result<()> {
        // Clipboard is restored inline in try_get_selected_ca
        Ok(())
    }
}

struct HotkeyManagerClone {
    current_ca: Option<SolanaAddress>,
    last_clipboard_text: Option<String>,
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
