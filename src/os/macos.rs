//! macOS backend.
//!
//!  * keys:    a CGEventTap on the main run loop (needs the Accessibility and Input Monitoring permissions)
//!  * layouts: Text Input Sources (Carbon TIS) - listing, selecting, and converting text between layouts with
//!             UCKeyTranslate. TIS must be used on the main thread, so other threads hop onto the main queue.
//!  * typing:  CGEvents carrying Unicode strings, marked with our user-data value so the tap ignores them
//!  * the application event loop (tao) lives here too: it drives the menu bar icon created in tray.rs
use super::{caret_plan, AfterConvert, KeyEvent, LayoutId, Step, K};
use crate::{lang::Lang, macplat};
use anyhow::{anyhow, Result};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    base::{CFType, CFTypeRef, TCFType},
    boolean::{CFBoolean, CFBooleanRef},
    data::{CFData, CFDataRef},
    dictionary::{CFDictionary, CFDictionaryRef},
    number::{CFNumber, CFNumberRef},
    runloop::{kCFRunLoopCommonModes, CFRunLoop},
    string::{CFString, CFStringRef},
};
use core_graphics::{
    event::{
        CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventType, EventField,
    },
    event_source::{CGEventSource, CGEventSourceStateID},
    window::copy_window_info,
};
use std::{
    collections::HashMap,
    ffi::c_void,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
    thread::sleep,
    time::{Duration, Instant},
};
use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS},
};

// ───────────────────────── system APIs ─────────────────────────

type OSStatus = i32;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyCurrentKeyboardInputSource() -> *const c_void;
    fn TISCreateInputSourceList(properties: *const c_void, include_all_installed: u8) -> *const c_void;
    fn TISGetInputSourceProperty(source: *const c_void, key: CFStringRef) -> *const c_void;
    fn TISSelectInputSource(source: *const c_void) -> OSStatus;
    static kTISPropertyInputSourceID: CFStringRef;
    static kTISPropertyInputSourceCategory: CFStringRef;
    static kTISPropertyInputSourceIsSelectCapable: CFStringRef;
    static kTISPropertyInputSourceLanguages: CFStringRef;
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    static kTISCategoryKeyboardInputSource: CFStringRef;
    fn UCKeyTranslate(
        key_layout_ptr: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: usize,
        actual_string_length: *mut usize,
        unicode_string: *mut u16,
    ) -> OSStatus;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapEnable(tap: *const c_void, enable: bool);
    fn CGEventSourceGetKeyboardType(source: *const c_void) -> u32;
}

#[link(name = "proc")]
extern "C" {
    fn proc_pidpath(pid: i32, buffer: *mut c_void, buffersize: u32) -> i32;
}

// ───────────────────────── main-thread hop ─────────────────────────

/// Run `f` on the main thread (TIS must not be used from other threads) and return its result.
/// On the main thread itself it simply runs inline.
fn on_main<R, F>(f: F) -> R
where
    R: Send + 'static,
    F: FnOnce() -> R + Send + 'static,
{
    if unsafe { libc::pthread_main_np() } != 0 {
        f()
    } else {
        dispatch::Queue::main().exec_sync(f)
    }
}

// ───────────────────────── input sources (layouts) ─────────────────────────

#[derive(Clone)]
struct Source {
    hash: LayoutId,
    lang: Option<Lang>,
}

unsafe fn prop_string(src: *const c_void, key: CFStringRef) -> Option<String> {
    let p = TISGetInputSourceProperty(src, key);
    if p.is_null() {
        None
    } else {
        Some(CFString::wrap_under_get_rule(p as CFStringRef).to_string())
    }
}

unsafe fn prop_bool(src: *const c_void, key: CFStringRef) -> bool {
    let p = TISGetInputSourceProperty(src, key);
    !p.is_null() && bool::from(CFBoolean::wrap_under_get_rule(p as CFBooleanRef))
}

unsafe fn prop_first_language(src: *const c_void) -> Option<String> {
    let p = TISGetInputSourceProperty(src, kTISPropertyInputSourceLanguages);
    if p.is_null() {
        return None;
    }
    let arr: CFArray<CFString> = CFArray::wrap_under_get_rule(p as CFArrayRef);
    arr.iter().next().map(|s| (*s).to_string())
}

unsafe fn uchr_bytes(src: *const c_void) -> Option<Vec<u8>> {
    let p = TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData);
    if p.is_null() {
        return None;
    }
    Some(CFData::wrap_under_get_rule(p as CFDataRef).bytes().to_vec())
}

/// Enabled, selectable keyboard input sources. MAIN THREAD ONLY.
unsafe fn sources_main() -> Vec<(CFType, Source)> {
    let list = TISCreateInputSourceList(std::ptr::null(), 0);
    if list.is_null() {
        return vec![];
    }
    let arr: CFArray<CFType> = CFArray::wrap_under_create_rule(list as CFArrayRef);
    let keyboard = CFString::wrap_under_get_rule(kTISCategoryKeyboardInputSource).to_string();
    let mut out = Vec::new();
    for item in arr.iter() {
        let src: CFType = (*item).clone();
        let p = src.as_CFTypeRef() as *const c_void;
        if prop_string(p, kTISPropertyInputSourceCategory).as_deref() != Some(keyboard.as_str())
            || !prop_bool(p, kTISPropertyInputSourceIsSelectCapable)
        {
            continue;
        }
        let Some(id) = prop_string(p, kTISPropertyInputSourceID) else { continue };
        let lang = prop_first_language(p).and_then(|t| macplat::lang_from_tis(&t));
        out.push((src, Source { hash: macplat::layout_id(&id), lang }));
    }
    out
}

static LANGS: Mutex<Option<HashMap<LayoutId, Option<Lang>>>> = Mutex::new(None);

fn remember(sources: &[Source]) {
    let mut g = LANGS.lock().unwrap_or_else(|p| p.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    for s in sources {
        m.insert(s.hash, s.lang);
    }
}

fn source_infos() -> Vec<Source> {
    let v: Vec<Source> = on_main(|| unsafe { sources_main().into_iter().map(|(_, s)| s).collect() });
    remember(&v);
    v
}

/// All keyboard layouts the user has enabled, in the order the system reports them.
pub fn layouts() -> Vec<LayoutId> {
    source_infos().into_iter().map(|s| s.hash).collect()
}

pub fn current_layout() -> LayoutId {
    let (id, lang) = on_main(|| unsafe {
        let p = TISCopyCurrentKeyboardInputSource();
        if p.is_null() {
            return (0, None);
        }
        let src = CFType::wrap_under_create_rule(p as CFTypeRef);
        let sp = src.as_CFTypeRef() as *const c_void;
        let id = prop_string(sp, kTISPropertyInputSourceID).map(|s| macplat::layout_id(&s)).unwrap_or(0);
        let lang = prop_first_language(sp).and_then(|t| macplat::lang_from_tis(&t));
        (id, lang)
    });
    if id != 0 {
        remember(&[Source { hash: id, lang }]);
    }
    id
}

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

pub fn layout_lang(id: LayoutId) -> Option<Lang> {
    let lookup = || LANGS.lock().unwrap_or_else(|p| p.into_inner()).as_ref().and_then(|m| m.get(&id).copied());
    match lookup() {
        Some(l) => l,
        None => {
            let _ = layouts();
            lookup().flatten()
        }
    }
}

pub fn installed_languages() -> Vec<Lang> {
    let mut v: Vec<Lang> = vec![];
    for l in layouts().into_iter().filter_map(layout_lang) {
        if !v.contains(&l) {
            v.push(l);
        }
    }
    v
}

/// Select an input source.
pub fn switch_to(id: LayoutId) -> Result<()> {
    let ok = on_main(move || unsafe {
        for (src, info) in sources_main() {
            if info.hash == id {
                return TISSelectInputSource(src.as_CFTypeRef() as *const c_void) == 0;
            }
        }
        false
    });
    if ok { Ok(()) } else { Err(anyhow!("could not select the input source")) }
}

// ───────────────────────── converting text between layouts ─────────────────────────

/// What the key at `code` types in a layout (`uchr` = its 'uchr' data) with the given modifier state.
fn translate_with(uchr: &[u8], code: u16, mods: u32, kbd: u32) -> Option<char> {
    let mut dead: u32 = 0;
    let mut len: usize = 0;
    let mut buf = [0u16; 4];
    let status = unsafe {
        UCKeyTranslate(
            uchr.as_ptr() as *const c_void,
            code,
            0, // kUCKeyActionDown
            mods,
            kbd,
            1, // kUCKeyTranslateNoDeadKeysMask
            &mut dead,
            buf.len(),
            &mut len,
            buf.as_mut_ptr(),
        )
    };
    if status != 0 || len != 1 {
        return None;
    }
    let c = char::from_u32(buf[0] as u32)?;
    if c.is_control() { None } else { Some(c) }
}

/// char -> (key code, modifier state) that produces it in this layout.
fn char_map(uchr: &[u8], kbd: u32) -> HashMap<char, (u16, u32)> {
    let mut m = HashMap::new();
    // unshifted first, so the plainest way to type a character wins
    for mods in [0, macplat::uc_modifier_state(true, false, false), macplat::uc_modifier_state(false, false, true), macplat::uc_modifier_state(true, false, true)] {
        for code in 0u16..=50 {
            if let Some(c) = translate_with(uchr, code, mods, kbd) {
                m.entry(c).or_insert((code, mods));
            }
        }
    }
    m
}

/// Text typed in layout `from` -> what the same key presses produce in layout `to`.
pub fn convert(text: &str, from: LayoutId, to: LayoutId) -> String {
    if from == to {
        return text.to_string();
    }
    let (fb, tb) = on_main(move || unsafe {
        let (mut f, mut t) = (None, None);
        for (src, info) in sources_main() {
            let p = src.as_CFTypeRef() as *const c_void;
            if info.hash == from {
                f = uchr_bytes(p);
            }
            if info.hash == to {
                t = uchr_bytes(p);
            }
        }
        (f, t)
    });
    let (Some(fb), Some(tb)) = (fb, tb) else {
        return text.to_string(); // an input method without a key layout: nothing to map
    };
    let kbd = unsafe { CGEventSourceGetKeyboardType(std::ptr::null()) };
    let from_map = char_map(&fb, kbd);
    let mut cache: HashMap<char, char> = HashMap::new();
    text.chars()
        .map(|c| {
            *cache.entry(c).or_insert_with(|| match from_map.get(&c) {
                Some(&(code, mods)) => translate_with(&tb, code, mods, kbd).unwrap_or(c),
                None => c,
            })
        })
        .collect()
}

/// The character a key press produces in the CURRENT layout.
fn translate_current(code: u16, mods: u32) -> String {
    on_main(move || unsafe {
        let p = TISCopyCurrentKeyboardInputSource();
        if p.is_null() {
            return String::new();
        }
        let src = CFType::wrap_under_create_rule(p as CFTypeRef);
        let data = TISGetInputSourceProperty(src.as_CFTypeRef() as *const c_void, kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return String::new();
        }
        let d = CFData::wrap_under_get_rule(data as CFDataRef);
        let kbd = CGEventSourceGetKeyboardType(std::ptr::null());
        translate_with(d.bytes(), code, mods, kbd).map(|c| c.to_string()).unwrap_or_default()
    })
}

// ───────────────────────── event tap (keys) ─────────────────────────

type Handler = Box<dyn Fn(KeyEvent) -> bool + Send + Sync>;
static HANDLER: OnceLock<Handler> = OnceLock::new();
static TAP_PORT: AtomicUsize = AtomicUsize::new(0);

fn synthetic_reset() -> KeyEvent {
    KeyEvent { down: true, key: K::Reset, text: String::new(), name: "" }
}

fn on_event(etype: CGEventType, event: &CGEvent) -> Option<CGEvent> {
    if matches!(etype, CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput) {
        // macOS switches a slow tap off: turn it back on
        let port = TAP_PORT.load(Ordering::SeqCst);
        if port != 0 {
            unsafe { CGEventTapEnable(port as *const c_void, true) };
        }
        return Some(event.clone());
    }
    if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == macplat::MARK {
        return Some(event.clone()); // our own synthetic input
    }
    let Some(h) = HANDLER.get() else { return Some(event.clone()) };

    let swallow = catch_unwind(AssertUnwindSafe(|| match etype {
        CGEventType::LeftMouseDown | CGEventType::RightMouseDown | CGEventType::OtherMouseDown => {
            h(synthetic_reset())
        }
        CGEventType::FlagsChanged => {
            let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let flags = event.get_flags();
            match macplat::modifier_kind(code) {
                Some(key) => {
                    let flag = match key {
                        K::Shift => CGEventFlags::CGEventFlagShift,
                        K::Ctrl => CGEventFlags::CGEventFlagControl,
                        K::Alt => CGEventFlags::CGEventFlagAlternate,
                        _ => CGEventFlags::CGEventFlagCommand,
                    };
                    h(KeyEvent { down: flags.contains(flag), key, text: String::new(), name: "" })
                }
                None => false,
            }
        }
        CGEventType::KeyDown | CGEventType::KeyUp => {
            let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let down = matches!(etype, CGEventType::KeyDown);
            let flags = event.get_flags();
            let key = macplat::classify(code);
            let command_or_control =
                flags.contains(CGEventFlags::CGEventFlagCommand) || flags.contains(CGEventFlags::CGEventFlagControl);
            let text = if down && key == K::Char && !command_or_control {
                let mods = macplat::uc_modifier_state(
                    flags.contains(CGEventFlags::CGEventFlagShift),
                    flags.contains(CGEventFlags::CGEventFlagAlphaShift),
                    flags.contains(CGEventFlags::CGEventFlagAlternate),
                );
                translate_current(code, mods)
            } else {
                String::new()
            };
            h(KeyEvent { down, key, text, name: macplat::key_name(code) })
        }
        _ => false,
    }))
    .unwrap_or(false);

    if swallow { None } else { Some(event.clone()) }
}

/// Create the tap and attach it to the MAIN run loop (so callbacks run on the main thread, where TIS is allowed).
fn install_event_tap() -> Result<()> {
    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
            CGEventType::LeftMouseDown,
            CGEventType::RightMouseDown,
            CGEventType::OtherMouseDown,
        ],
        |_proxy, etype, event| on_event(etype, event),
    )
    .map_err(|_| {
        anyhow!(
            "Retypr could not listen to the keyboard.\n\nGrant it the Accessibility and Input Monitoring permissions in \
             System Settings > Privacy & Security, then start Retypr again."
        )
    })?;

    let source = tap
        .mach_port
        .create_runloop_source(0)
        .map_err(|_| anyhow!("could not create the run loop source for the keyboard tap"))?;
    CFRunLoop::get_main().add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();
    TAP_PORT.store(tap.mach_port.as_concrete_TypeRef() as usize, Ordering::SeqCst);
    std::mem::forget(tap); // the tap must live as long as the process
    Ok(())
}

/// Ask for the permissions the app needs (shows the system prompts the first time).
fn request_permissions() {
    unsafe {
        let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
        let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        let _ = AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef() as *const c_void);
        let _ = CGRequestListenEventAccess();
    }
}

// ───────────────────────── the focused application ─────────────────────────

static CUR_APP: Mutex<String> = Mutex::new(String::new());

/// Executable name of the app owning the frontmost normal window, lowercase ("safari", "electron"...).
fn frontmost_app_name() -> String {
    // kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID
    let Some(list) = copy_window_info(1 | 16, 0) else { return String::new() };
    let k_layer = CFString::from_static_string("kCGWindowLayer");
    let k_pid = CFString::from_static_string("kCGWindowOwnerPID");
    for item in list.iter() {
        let dict = unsafe { CFDictionary::<*const c_void, *const c_void>::wrap_under_get_rule(*item as CFDictionaryRef) };
        let number = |key: &CFString| -> Option<i32> {
            dict.find(key.as_concrete_TypeRef() as *const c_void)
                .and_then(|v| unsafe { CFNumber::wrap_under_get_rule(*v as CFNumberRef) }.to_i32())
        };
        if number(&k_layer) != Some(0) {
            continue; // menu bar, Dock, floating panels...
        }
        let Some(pid) = number(&k_pid) else { continue };
        let mut buf = [0u8; 4096];
        let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
        if n > 0 {
            return macplat::exe_name_from_path(&String::from_utf8_lossy(&buf[..n as usize]));
        }
    }
    String::new()
}

/// Exe name of the focused application (kept fresh by a background poller).
pub fn foreground_app() -> String {
    CUR_APP.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

fn start_app_poller() {
    std::thread::spawn(|| loop {
        let name = frontmost_app_name();
        let changed = {
            let mut g = CUR_APP.lock().unwrap_or_else(|p| p.into_inner());
            let changed = *g != name;
            if changed {
                *g = name;
            }
            changed
        };
        if changed {
            if let Some(h) = HANDLER.get() {
                let _ = catch_unwind(AssertUnwindSafe(|| h(synthetic_reset())));
            }
        }
        sleep(Duration::from_millis(250));
    });
}

// ───────────────────────── posting events (typing) ─────────────────────────

fn event_source() -> Result<CGEventSource> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("could not create an event source (is the Accessibility permission granted?)"))
}

fn post(ev: CGEvent, flags: CGEventFlags) {
    ev.set_flags(flags);
    ev.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, macplat::MARK);
    ev.post(CGEventTapLocation::HID);
}

fn post_key(src: &CGEventSource, code: u16, down: bool, flags: CGEventFlags) {
    if let Ok(ev) = CGEvent::new_keyboard_event(src.clone(), code, down) {
        post(ev, flags);
    }
}

/// Type text as Unicode (independent of the layout), a few characters per event.
fn post_text(src: &CGEventSource, text: &str) {
    let chars: Vec<char> = text.chars().collect();
    for chunk in chars.chunks(10) {
        let s: String = chunk.iter().collect();
        for down in [true, false] {
            if let Ok(ev) = CGEvent::new_keyboard_event(src.clone(), 0, down) {
                if down {
                    ev.set_string(&s);
                }
                post(ev, CGEventFlags::empty());
            }
        }
        sleep(Duration::from_millis(1));
    }
}

fn command_chord(src: &CGEventSource, code: u16) {
    post_key(src, code, true, CGEventFlags::CGEventFlagCommand);
    post_key(src, code, false, CGEventFlags::CGEventFlagCommand);
    sleep(Duration::from_millis(2));
}

/// Erase `erase` characters, type `text`, switch the layout.
pub fn replace_typed(erase: usize, text: &str, to: LayoutId) -> Result<()> {
    // Switch first, so whatever the user types next is already in the target layout.
    if let Err(e) = switch_to(to) {
        log::warn!("layout switch failed: {e:#}");
    }
    let src = event_source()?;
    for n in 0..erase {
        post_key(&src, macplat::KC_BACKSPACE, true, CGEventFlags::empty());
        post_key(&src, macplat::KC_BACKSPACE, false, CGEventFlags::empty());
        if n % 200 == 199 {
            sleep(Duration::from_millis(1));
        }
    }
    post_text(&src, text);
    Ok(())
}

/// Selection conversion: copy the selection, hand it to `f`, paste the result, fix up selection/caret
/// (see os::caret_plan), restore the clipboard.
pub fn convert_selection<F>(after: AfterConvert, f: F) -> Result<()>
where
    F: FnOnce(&str, LayoutId) -> Option<(String, LayoutId)>,
{
    let src = event_source()?;
    let mut cb = arboard::Clipboard::new()?;
    let saved = cb.get_text().ok();
    cb.set_text("")?;

    command_chord(&src, macplat::KC_C);

    // poll quickly at first, then more slowly (a huge selection can take a while to render)
    let mut sel = String::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        if let Ok(t) = cb.get_text() {
            if !t.is_empty() {
                sel = t;
                break;
            }
        }
        let fast = started.elapsed() < Duration::from_millis(150);
        sleep(Duration::from_millis(if fast { 4 } else { 20 }));
    }

    let mut restore_after_ms = 0u64;
    if !sel.is_empty() {
        let cur = current_layout();
        if let Some((conv, target)) = f(&sel, cur) {
            let n = conv.replace("\r\n", "\n").chars().count();
            cb.set_text(conv)?;
            if let Err(e) = switch_to(target) {
                log::warn!("layout switch failed: {e:#}");
            }
            command_chord(&src, macplat::KC_V);

            let mut shift = false;
            let flags = |shift: bool| if shift { CGEventFlags::CGEventFlagShift } else { CGEventFlags::empty() };
            let mut sent = 0usize;
            for step in caret_plan(after, n) {
                match step {
                    Step::ShiftDown => shift = true,
                    Step::ShiftUp => shift = false,
                    Step::Left => {
                        post_key(&src, macplat::KC_LEFT, true, flags(shift));
                        post_key(&src, macplat::KC_LEFT, false, flags(shift));
                    }
                    Step::Right => {
                        post_key(&src, macplat::KC_RIGHT, true, flags(shift));
                        post_key(&src, macplat::KC_RIGHT, false, flags(shift));
                    }
                }
                sent += 1;
                if sent % 500 == 0 {
                    sleep(Duration::from_millis(1));
                }
            }
            restore_after_ms = (400 + n as u64 / 20).min(3000);
        }
    }

    if let Some(s) = saved {
        if restore_after_ms == 0 {
            cb.set_text(s)?;
        } else {
            std::thread::spawn(move || {
                sleep(Duration::from_millis(restore_after_ms));
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_text(s);
                }
            });
        }
    }
    Ok(())
}

/// Not needed on macOS (a lone Option/Command tap does nothing harmful).
pub fn mask_menu() {}

// ───────────────────────── application loop, dialogs ─────────────────────────

static PROXY: OnceLock<Mutex<EventLoopProxy<()>>> = OnceLock::new();

/// Leave the event loop (`run` ends and the process exits).
pub fn quit() {
    if let Some(p) = PROXY.get() {
        let _ = p.lock().unwrap_or_else(|e| e.into_inner()).send_event(());
    }
}

fn alert(text: &str) {
    let script = format!("display alert \"Retypr\" message \"{}\"", macplat::osascript_escape(text));
    let _ = std::process::Command::new("osascript").arg("-e").arg(script).status();
}

pub fn fatal(msg: &str) {
    alert(msg);
}

pub fn warn(msg: &str) {
    alert(msg);
}

/// Install the key tap and run the application event loop (it drives the menu bar icon).
/// `init` runs once inside the loop (create the tray there), `tick` every ~150 ms, `on_exit` before the process ends.
pub fn run<F, I>(handler: F, init: I, tick: fn(), on_exit: fn()) -> Result<()>
where
    F: Fn(KeyEvent) -> bool + Send + Sync + 'static,
    I: FnOnce() -> Result<()> + 'static,
{
    HANDLER.set(Box::new(handler)).map_err(|_| anyhow!("already running"))?;
    request_permissions();
    install_event_tap()?;
    start_app_poller();

    let mut builder = EventLoopBuilder::<()>::with_user_event();
    builder.with_activation_policy(ActivationPolicy::Accessory); // no Dock icon
    let event_loop = builder.build();
    let _ = PROXY.set(Mutex::new(event_loop.create_proxy()));

    let mut init = Some(init);
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(150));
        match event {
            Event::NewEvents(StartCause::Init) => {
                if let Some(i) = init.take() {
                    if let Err(e) = i() {
                        fatal(&format!("{e:#}"));
                        *flow = ControlFlow::Exit;
                    }
                }
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => tick(),
            Event::UserEvent(()) => *flow = ControlFlow::Exit,
            Event::LoopDestroyed => on_exit(),
            _ => {}
        }
    })
}

