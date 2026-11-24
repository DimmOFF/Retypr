# Changelog

## 0.2.6
* **Freeze of winit-based apps (e.g. PhotoCraft) found and worked around.** Apps built on winit 0.30 freeze for good
  when the keyboard layout is switched from outside (a known winit bug fixed in winit 0.31, rust-windowing/winit#4582);
  any layout switcher triggers it. On Windows, Retypr no longer sends layout-change requests to windows of class
  `Window Class` (winit's default class name) - the text is still corrected. Setting
  `skip_layout_switch_for_winit_apps` (on by default, *Settings > Apps*).
* Safety net: if the window that received a layout-change request stops responding for ~10 s, its app is added to
  `no_layout_switch_apps` automatically and a message explains why.

## 0.2.5
* Be gentler with sensitive applications (one reported a hard freeze when auto-correction fired):
  * a layout-change request is sent only when the layout really has to change, to the window that owns the keyboard
    focus (a second request only if the focus belongs to another thread), and repeated at most once;
  * corrected text is typed in small paced batches instead of one burst.
* New per-app list **Don't switch the layout** (`no_layout_switch_apps`, Settings > Apps): the text is still
  corrected but the layout is left alone - a quick way to find out whether an app dislikes layout-change requests.

## 0.2.4
* Fix sounds that sometimes stopped playing until Retypr was restarted. The audio thread used to give up for good
  if no output device existed at start-up, and it never noticed a changed default device or a stream that had died
  (sleep/resume, a Bluetooth headset powering down, an audio-service restart). The output is now opened lazily,
  re-checked before every sound, reopened when the default device changes or after 30 s of idleness, and retried once
  on failure. Sound problems are written to the log instead of being swallowed.

## 0.2.3
* The selection option is positive again: **Select after converting** (`select_after_convert`), **off by default**.
  It replaces `deselect_after_convert` (the old key is ignored). With it off, converted text is left unselected and
  pressing the hotkey again no longer cycles through the layouts; turn it on to get the cycling back.

## 0.2.2
* macOS: fix the start-up error "single-instance: Read-only file system". Apps started from Finder have the read-only
  root `/` as their working directory, and the crate we used wrote its lock file there. The guard is now an `flock`
  on a file in the per-user temp folder (macOS and Linux); Windows keeps the named mutex.

## 0.2.1
* macOS: fix the build against `tao 0.30` (the activation policy is set on the event loop, not on its builder).

## 0.2.0

### macOS (new)
* Native macOS backend: global keys through an event tap, switching and converting between the **input sources
  installed in the system** (Text Input Sources + UCKeyTranslate), typing through CGEvents, selection conversion
  through the clipboard, focused-app detection for per-app rules.
* Menu bar icon with the same menu as the Windows tray, the settings window, and "Start at login" (a LaunchAgent).
* Default hotkeys for Mac keyboards (no Pause key): `Ctrl+Alt+Z`, `Ctrl+Alt+Shift+Z`, `Ctrl+Alt+A`, `Ctrl+Alt+Shift+A`.
* `build-macos.sh` builds a universal `.app` and `.dmg`.

### Added
* **Portable mode**: settings, dictionaries and sounds live in a `data` folder next to the executable; config paths may
  be relative; the folder can be moved anywhere.
* **Any layouts**: auto-switching works with whatever keyboard layouts are installed in the system, not a fixed
  language pair. Per-language dictionaries (`dict/<code>.txt`).
* **Dictionary manager** in *Settings > Dictionaries*: download or update a word list per installed layout, enable or
  disable each language, or all of them at once.
* **Tray icon states**: blue, gray, and a diagonal blue/gray split when auto-switching is paused for the focused app.
* **Default sounds** (shipped as files in `data/sounds`) and a default volume of 30%.
* Absolute sound paths in the settings; *Browse* opens the folder of the current sound.
* Selection conversion options: remove the selection afterwards, caret position, and a size limit for restoring it.
* Settings window, hotkey editor, per-app rules, themes; `Retypr.exe` (capital R); GitHub link in *About*.
* GitHub Actions for CI and releases; English documentation.

### Changed
* Faster conversion of long selections (memoized layout conversion, fast clipboard polling, no fixed pause,
  background clipboard restore).
* Minimum word length defaults to 2; two-letter words are converted only when a dictionary confirms it.

### Fixed
* Crash when clicking *Auto-switching*, *Sounds* or *Start with Windows* in the tray menu.
* The layout was not always switched after auto-correction (it is now switched first, targets the window that has the
  keyboard focus, and is verified in the background).
* The settings window is brought to the front instead of being ignored, and closes when Retypr exits.

## 0.1.0
* Initial version: automatic layout correction, hotkeys, tray icon, settings file.
