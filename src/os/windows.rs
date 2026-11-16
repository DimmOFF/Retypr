//! Native Windows backend: WH_KEYBOARD_LL / WH_MOUSE_LL hooks, SendInput, keyboard layouts (HKL).
use super::{caret_plan, AfterConvert, KeyEvent, LayoutId, Step, K};
use crate::hotkey::{DIGITS, FKEYS, LETTERS};
use crate::lang::Lang;
use anyhow::{anyhow, Result};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicIsize, Ordering},
        Mutex, OnceLock,
    },
    thread::sleep,
    time::Duration,
};
use windows::{
    core::{w, HSTRING, PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, BOOL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{
            Shell::{IsUserAnAdmin, ShellExecuteW},
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, GetKeyState, GetKeyboardLayout, GetKeyboardLayoutList,
                MapVirtualKeyExW, SendInput, ToUnicodeEx, VkKeyScanExW, HKL, INPUT, INPUT_0,
                INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, MAP_VIRTUAL_KEY_TYPE, VIRTUAL_KEY,
            },
            WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, EnumWindows, GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GUITHREADINFO, GetWindowTextW, IsIconic,
                GetWindowThreadProcessId, MessageBoxW, PostMessageW, PostQuitMessage, SetTimer, SetWindowsHookExW,
                TranslateMessage, MB_ICONERROR, MB_ICONWARNING, MB_OK,
                HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WH_MOUSE_LL,
                WM_INPUTLANGCHANGEREQUEST, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
                WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP, SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, WM_CLOSE,
            },
        },
    },
};

/// Marker in dwExtraInfo: this is how the hook tells our synthetic input from real input.
const MARK: usize = 0x5254_5950; // "RTYP"

const VK_BACK: u16 = 0x08;
const VK_CONTROL: u16 = 0x11;
const VK_LSHIFT: u16 = 0xA0;
const VK_RSHIFT: u16 = 0xA1;
const VK_LEFT: u16 = 0x25;
const VK_RIGHT: u16 = 0x27;
const VK_C: u16 = 0x43;
const VK_V: u16 = 0x56;

type Handler = Box<dyn Fn(KeyEvent) -> bool + Send + Sync>;
static HANDLER: OnceLock<Handler> = OnceLock::new();
static LAST_FG: AtomicIsize = AtomicIsize::new(0);
/// Exe name of the focused window (lowercase). Updated only when the window changes, not on every key.
static CUR_EXE: Mutex<String> = Mutex::new(String::new());

fn exe_of(hwnd: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
        if pid == 0 {
            return String::new();
        }
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        if r.is_err() {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().unwrap_or("").to_lowercase()
    }
}

/// true if the focused window changed (CUR_EXE is refreshed then).
fn refresh_fg(fg: HWND) -> bool {
    let v = fg.0 as isize;
    if LAST_FG.swap(v, Ordering::Relaxed) != v {
        *CUR_EXE.lock().unwrap_or_else(|p| p.into_inner()) = exe_of(fg);
        true
    } else {
        false
    }
}

/// Exe name of the focused window, e.g. "code.exe".
pub fn foreground_app() -> String {
    CUR_EXE.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

// ───────────────────────── hooks ─────────────────────────

unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == 0 {
        // HC_ACTION
        let swallow = catch_unwind(AssertUnwindSafe(|| on_key(wparam, lparam))).unwrap_or(false);
        if swallow {
            return LRESULT(1);
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == 0 {
        if let Some(h) = HANDLER.get() {
            if matches!(wparam.0 as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
                refresh_fg(GetForegroundWindow());
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    h(KeyEvent { down: true, key: K::Reset, text: String::new(), name: "" })
                }));
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

unsafe fn on_key(wparam: WPARAM, lparam: LPARAM) -> bool {
    let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    if kb.dwExtraInfo == MARK {
        return false; // our own synthetic input
    }
    let Some(h) = HANDLER.get() else { return false };

    let msg = wparam.0 as u32;
    let down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
    let up = matches!(msg, WM_KEYUP | WM_SYSKEYUP);
    if !down && !up {
        return false;
    }

    // The focused window changed: the current word is no longer relevant
    let fg = GetForegroundWindow();
    if refresh_fg(fg) {
        h(KeyEvent { down: true, key: K::Reset, text: String::new(), name: "" });
    }

    let key = match kb.vkCode {
        0x10 | 0xA0 | 0xA1 => K::Shift,
        0x11 | 0xA2 | 0xA3 => K::Ctrl,
        0x12 | 0xA4 | 0xA5 => K::Alt,
        0x5B | 0x5C => K::Meta,
        0x08 => K::Backspace,
        0x20 => K::Space,
        // Tab, Enter, Esc, PgUp..PgDn, End, Home, arrows, Ins, Del
        0x09 | 0x0D | 0x1B | 0x21..=0x28 | 0x2D | 0x2E => K::Reset,
        _ => K::Char,
    };
    let text = if down && key == K::Char { translate(kb.vkCode, kb.scanCode) } else { String::new() };
    h(KeyEvent { down, key, text, name: vk_name(kb.vkCode) })
}

/// Virtual-key code -> canonical key name used by hotkeys.
fn vk_name(vk: u32) -> &'static str {
    match vk {
        0x13 => "pause",
        0x03 => "break", // VK_CANCEL: Ctrl+Pause
        0x91 => "scrolllock",
        0x14 => "capslock",
        0x2D => "insert",
        0x2E => "delete",
        0x24 => "home",
        0x23 => "end",
        0x21 => "pageup",
        0x22 => "pagedown",
        0x2C => "printscreen",
        0x5D => "menu",
        0x20 => "space",
        0x08 => "backspace",
        0x09 => "tab",
        0x0D => "enter",
        0x1B => "escape",
        0x25 => "left",
        0x27 => "right",
        0x26 => "up",
        0x28 => "down",
        0xC0 => "grave",
        0x70..=0x87 => FKEYS[(vk - 0x70) as usize],
        0x30..=0x39 => &DIGITS[(vk - 0x30) as usize..][..1],
        0x41..=0x5A => &LETTERS[(vk - 0x41) as usize..][..1],
        _ => "",
    }
}

/// vk -> the character in the focused window's current layout (without side effects on dead keys).
unsafe fn translate(vk: u32, scan: u32) -> String {
    let (_, tid, _) = input_target();
    let hkl = GetKeyboardLayout(tid);

    let mut st = [0u8; 256];
    if GetAsyncKeyState(0x10) as u16 & 0x8000 != 0 {
        st[0x10] = 0x80; // Shift
    }
    if GetKeyState(0x14) & 1 != 0 {
        st[0x14] = 1; // CapsLock
    }
    let mut buf = [0u16; 8];
    // flag 0x4: do not change the dead-key state (Windows 10 1607+)
    let n = ToUnicodeEx(vk, scan, &st, &mut buf, 0x4, hkl);
    if n > 0 {
        String::from_utf16_lossy(&buf[..n as usize])
    } else {
        String::new()
    }
}

/// Installs the hooks and runs the message loop. Blocks the thread.
pub fn run<F>(handler: F) -> Result<()>
where
    F: Fn(KeyEvent) -> bool + Send + Sync + 'static,
{
    HANDLER.set(Box::new(handler)).map_err(|_| anyhow!("hook already installed"))?;
    unsafe {
        let m = GetModuleHandleW(PCWSTR::null())?;
        let inst = HINSTANCE(m.0);
        let _kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), inst, 0)?;
        let _ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), inst, 0)?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

// ───────────────────────── layouts ─────────────────────────

fn hkl(id: LayoutId) -> HKL {
    HKL(id as *mut core::ffi::c_void)
}

/// Layout of the focused window.
/// (foreground window, thread that owns the keyboard focus, window that has the focus).
/// The focus window can belong to another thread than the top-level window (terminals, some browsers), and the
/// keyboard layout is per thread, so layouts must be read from - and switched for - the FOCUS thread.
unsafe fn input_target() -> (HWND, u32, HWND) {
    let fg = GetForegroundWindow();
    let fg_tid = GetWindowThreadProcessId(fg, None);
    let mut gi = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
    let _ = GetGUIThreadInfo(fg_tid, &mut gi);
    let focus = if gi.hwndFocus.0.is_null() { fg } else { gi.hwndFocus };
    let tid = if focus == fg { fg_tid } else { GetWindowThreadProcessId(focus, None) };
    (fg, tid, focus)
}

pub fn current_layout() -> LayoutId {
    unsafe {
        let (_, tid, _) = input_target();
        GetKeyboardLayout(tid).0 as isize
    }
}

/// All layouts installed in the system (in system order).
pub fn layouts() -> Vec<LayoutId> {
    unsafe {
        let mut buf = [HKL::default(); 64];
        let n = GetKeyboardLayoutList(Some(&mut buf));
        buf[..n.max(0) as usize].iter().map(|h| h.0 as isize).collect()
    }
}

/// The next layout in the cycle.
pub fn next_layout(id: LayoutId) -> LayoutId {
    let l = layouts();
    if l.len() < 2 {
        return id;
    }
    match l.iter().position(|&x| x == id) {
        Some(i) => l[(i + 1) % l.len()],
        None => l[0],
    }
}

/// Language of a layout (primary LANGID from the low word of the HKL). Works for any installed layout.
pub fn layout_lang(id: LayoutId) -> Option<Lang> {
    Some(Lang::from_langid((id as usize & 0xFFFF) as u16))
}

/// Languages of all installed layouts, in system order, without duplicates.
pub fn installed_languages() -> Vec<Lang> {
    let mut v: Vec<Lang> = vec![];
    for l in layouts().into_iter().filter_map(layout_lang) {
        if !v.contains(&l) {
            v.push(l);
        }
    }
    v
}

/// Translate text typed in layout `from` into what the same key presses would produce in layout `to`.
pub fn convert(text: &str, from: LayoutId, to: LayoutId) -> String {
    if from == to {
        return text.to_string();
    }
    let (hf, ht) = (hkl(from), hkl(to));
    text.chars().map(|c| convert_char(c, hf, ht)).collect()
}

fn convert_char(c: char, hf: HKL, ht: HKL) -> char {
    let mut u = [0u16; 2];
    if c.encode_utf16(&mut u).len() != 1 {
        return c;
    }
    unsafe {
        let r = VkKeyScanExW(u[0], hf);
        if r == -1 {
            return c; // this character cannot be typed in the source layout
        }
        let r = r as u16;
        let (vk, sh) = ((r & 0xFF) as u32, (r >> 8) as u8);
        if vk == 0xFF {
            return c;
        }
        let sc = MapVirtualKeyExW(vk, MAP_VIRTUAL_KEY_TYPE(0), ht); // MAPVK_VK_TO_VSC
        let mut st = [0u8; 256];
        if sh & 1 != 0 { st[0x10] = 0x80; } // Shift
        if sh & 2 != 0 { st[0x11] = 0x80; } // Ctrl
        if sh & 4 != 0 { st[0x12] = 0x80; } // Alt (together with Ctrl = AltGr)
        let mut buf = [0u16; 4];
        let n = ToUnicodeEx(vk, sc, &st, &mut buf, 0x4, ht);
        if n == 1 {
            char::from_u32(buf[0] as u32).unwrap_or(c)
        } else {
            c
        }
    }
}

/// Switch the focused window to a specific layout.
fn post_layout_request(hwnd: HWND, id: LayoutId) -> Result<()> {
    unsafe {
        PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST, WPARAM(0), LPARAM(id))?;
    }
    Ok(())
}

/// Ask the focused window (and its top-level window) to switch to layout `id`. Asynchronous: the target thread
/// handles the request a moment later.
pub fn switch_to(id: LayoutId) -> Result<()> {
    let (fg, focus) = unsafe {
        let (fg, _, focus) = input_target();
        (fg, focus)
    };
    let first = post_layout_request(fg, id);
    if focus != fg {
        let _ = post_layout_request(focus, id);
    }
    first
}

/// Verify (in the background, without delaying anything) that the layout really changed and repeat the request
/// if not: some apps are slow, or ignore the first request.
fn ensure_layout_later(id: LayoutId) {
    std::thread::spawn(move || {
        for attempt in 0..3 {
            for _ in 0..6 {
                if current_layout() == id {
                    return;
                }
                sleep(Duration::from_millis(15));
            }
            log::debug!("layout request #{attempt} is not applied yet, repeating it");
            let _ = switch_to(id);
        }
        if current_layout() != id {
            log::warn!("the layout did not switch to {id:#x}");
        }
    });
}

// ───────────────────────── input ─────────────────────────

fn ki(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: KEYBD_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: MARK,
            },
        },
    }
}
fn vk(vk: u16, up: bool) -> INPUT {
    ki(vk, 0, if up { 0x0002 } else { 0 }) // KEYEVENTF_KEYUP
}
fn vk_ext(vk: u16, up: bool) -> INPUT {
    ki(vk, 0, 0x0001 | if up { 0x0002 } else { 0 }) // KEYEVENTF_EXTENDEDKEY (arrow keys)
}
fn uni(unit: u16, up: bool) -> INPUT {
    ki(0, unit, 0x0004 | if up { 0x0002 } else { 0 }) // KEYEVENTF_UNICODE
}

fn send(inputs: &[INPUT]) {
    for chunk in inputs.chunks(120) {
        unsafe {
            SendInput(chunk, std::mem::size_of::<INPUT>() as i32);
        }
        sleep(Duration::from_millis(2));
    }
}

/// Erase `erase` characters, type `text` (Unicode, independent of the layout), switch the layout.
pub fn replace_typed(erase: usize, text: &str, to: LayoutId) -> Result<()> {
    // Switch the layout FIRST. The text below is typed as Unicode, so it does not depend on the layout, but
    // whatever the user types next must already be in the target layout. Doing it last left a window in which
    // fast typing went out in the old layout.
    if let Err(e) = switch_to(to) {
        log::warn!("layout switch request failed: {e:#}");
    }
    let mut v = Vec::with_capacity(erase * 2 + text.len() * 2);
    for _ in 0..erase {
        v.push(vk(VK_BACK, false));
        v.push(vk(VK_BACK, true));
    }
    for u in text.encode_utf16() {
        v.push(uni(u, false));
        v.push(uni(u, true));
    }
    send(&v);
    ensure_layout_later(to);
    Ok(())
}

/// Selection conversion: copy the selection, hand it to `f`, paste the result,
/// fix up selection/caret (see caret_plan), restore the clipboard.
/// `f(selected, current_layout) -> Some((new_text, target_layout))`.
pub fn convert_selection<F>(after: AfterConvert, f: F) -> Result<()>
where
    F: FnOnce(&str, LayoutId) -> Option<(String, LayoutId)>,
{
    // The user is holding Shift (Shift+Pause); otherwise the app would see Ctrl+Shift+C
    send(&[vk(VK_LSHIFT, true), vk(VK_RSHIFT, true)]);

    let mut cb = arboard::Clipboard::new()?;
    let saved = cb.get_text().ok();
    cb.set_text("")?;

    send(&[vk(VK_CONTROL, false), vk(VK_C, false), vk(VK_C, true), vk(VK_CONTROL, true)]);

    let mut sel = String::new();
    for _ in 0..10 {
        sleep(Duration::from_millis(50));
        if let Ok(t) = cb.get_text() {
            if !t.is_empty() {
                sel = t;
                break;
            }
        }
    }

    if !sel.is_empty() {
        let cur = current_layout();
        if let Some((conv, target)) = f(&sel, cur) {
            cb.set_text(conv.clone())?;
            if let Err(e) = switch_to(target) {
                log::warn!("layout switch request failed: {e:#}");
            }
            send(&[vk(VK_CONTROL, false), vk(VK_V, false), vk(VK_V, true), vk(VK_CONTROL, true)]);
            sleep(Duration::from_millis(200)); // give the app time to read the clipboard
            ensure_layout_later(target);

            // caret position / keep or drop the selection (see os::caret_plan)
            let n = conv.replace("\r\n", "\n").chars().count();
            let mut v = Vec::new();
            for step in caret_plan(after, n) {
                match step {
                    Step::ShiftDown => v.push(vk(VK_LSHIFT, false)),
                    Step::ShiftUp => v.push(vk(VK_LSHIFT, true)),
                    Step::Left => {
                        v.push(vk_ext(VK_LEFT, false));
                        v.push(vk_ext(VK_LEFT, true));
                    }
                    Step::Right => {
                        v.push(vk_ext(VK_RIGHT, false));
                        v.push(vk_ext(VK_RIGHT, true));
                    }
                }
            }
            if !v.is_empty() {
                send(&v);
            }
        }
    }
    if let Some(s) = saved {
        cb.set_text(s)?;
    }
    Ok(())
}

// ───────────────────────── application ─────────────────────────

/// After a hotkey that includes Alt/Win: "mask" the lone Alt/Win tap with an invisible key,
/// otherwise the app would open its menu (Alt) or the Start menu (Win).
pub fn mask_menu() {
    send(&[vk(0xE8, false), vk(0xE8, true)]); // 0xE8 is an unassigned code, the same trick AutoHotkey uses
}

/// Leave the message loop (`run` returns).
pub fn quit() {
    unsafe { PostQuitMessage(0) };
}

/// Show an error message box (the release build has no console).
pub fn fatal(msg: &str) {
    unsafe {
        MessageBoxW(HWND::default(), &HSTRING::from(msg), w!("Retypr"), MB_OK | MB_ICONERROR);
    }
}

/// Show a warning message box (non-critical).
pub fn warn(msg: &str) {
    unsafe {
        MessageBoxW(HWND::default(), &HSTRING::from(msg), w!("Retypr"), MB_OK | MB_ICONWARNING);
    }
}

/// Is the process running as administrator? (needed for the hook to see elevated windows)
pub fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

/// Start a copy of the program as administrator (shows a UAC prompt). The caller must then exit;
/// the new copy (flag --restarted) waits until the single-instance lock is released.
pub fn restart_elevated() -> Result<()> {
    let exe = std::env::current_exe()?;
    let r = unsafe {
        ShellExecuteW(
            HWND::default(),
            w!("runas"),
            &HSTRING::from(exe.to_string_lossy().as_ref()),
            w!("--restarted"),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if r.0 as isize > 32 {
        Ok(())
    } else {
        Err(anyhow!("could not restart as administrator (cancelled or failed, code {})", r.0 as isize))
    }
}

static TIMER_CB: OnceLock<fn()> = OnceLock::new();

unsafe extern "system" fn timer_proc(_h: HWND, _msg: u32, _id: usize, _time: u32) {
    if let Some(f) = TIMER_CB.get() {
        let _ = catch_unwind(AssertUnwindSafe(|| f()));
    }
}

/// Call `f` every `ms` milliseconds in the main thread (from the message loop). Used to refresh the tray.
pub fn every(ms: u32, f: fn()) {
    let _ = TIMER_CB.set(f);
    unsafe {
        SetTimer(HWND::default(), 1, ms, Some(timer_proc));
    }
}

// ───────────────────────── finding / focusing / closing a window by title ─────────────────────────

static FOUND_HWND: AtomicIsize = AtomicIsize::new(0);
static SEARCH_TITLE: Mutex<String> = Mutex::new(String::new());

unsafe extern "system" fn enum_proc(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    let mut buf = [0u16; 256];
    let n = GetWindowTextW(hwnd, &mut buf);
    if n > 0 {
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        if title == *SEARCH_TITLE.lock().unwrap_or_else(|p| p.into_inner()) {
            FOUND_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            return BOOL(0); // found: stop enumerating
        }
    }
    BOOL(1)
}

fn find_window(title: &str) -> Option<HWND> {
    FOUND_HWND.store(0, Ordering::SeqCst);
    *SEARCH_TITLE.lock().unwrap_or_else(|p| p.into_inner()) = title.to_string();
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(0));
    }
    match FOUND_HWND.load(Ordering::SeqCst) {
        0 => None,
        v => Some(HWND(v as *mut core::ffi::c_void)),
    }
}

/// Bring the top-level window with exactly this title to the front (restoring it if minimized).
/// Returns false if there is no such window.
pub fn focus_window(title: &str) -> bool {
    let Some(h) = find_window(title) else { return false };
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        // A synthetic key tap satisfies Windows' "foreground lock", so SetForegroundWindow is honored.
        send(&[vk(0xE8, false), vk(0xE8, true)]);
        let _ = SetForegroundWindow(h);
    }
    true
}

/// Ask the window with this title to close (WM_CLOSE). Does nothing if there is none.
pub fn close_window(title: &str) {
    if let Some(h) = find_window(title) {
        unsafe {
            let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}
