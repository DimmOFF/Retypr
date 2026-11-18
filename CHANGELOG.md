# Changelog

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
