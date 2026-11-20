# Retypr

A keyboard layout switcher in the spirit of Punto Switcher: it fixes text typed in the wrong layout, either
automatically or with hotkeys. Written in Rust.

**Windows and macOS are supported** (macOS support is new in 0.2.0 and has seen far less testing); Linux is
experimental. See [Platform status](#platform-status).

## Features

* **Automatic correction** - when you finish a word with Space, a word typed in the wrong layout is replaced
  (for example `ghbdtn` becomes `привет`) and the layout is switched.
* **Works with any layouts installed in Windows** - nothing is hard-wired to a particular language pair.
* **Hotkeys** (all configurable):

  | Default | Action |
  |---|---|
  | `Pause` | Convert the last word. Press again to cycle through **all** installed layouts. |
  | `Shift+Pause` | Convert the selected text. With *Select after converting* on, press it again to cycle through all layouts. |
  | `Alt+Pause` | Turn auto-switching on or off. |
  | `Alt+Shift+Pause` | Turn auto-switching on or off for the current application. |

* **Per-application rules** - keep auto-switching off in IDEs and terminals, or switch Retypr off completely in games.
* **Dictionaries** - download word lists per language in *Settings > Dictionaries*, enable or disable each one.
* **Sounds** - a sound for manual conversion, automatic correction and selection conversion (default sounds are shipped in `data/sounds`).
* **Tray icon** that shows the state at a glance (see below) and a modern settings window.
* **Portable** - settings live next to `Retypr.exe`.

## Getting started

1. Download the release zip, unpack it anywhere and run `Retypr.exe`. A tray icon appears.
2. Right-click the tray icon > **Settings...** > **Dictionaries**, then **Download** a word list for each language
   you type in. Detection works best with dictionaries. English, Russian and Ukrainian also have built-in rules,
   so they work (less accurately) without a dictionary.
3. Optional: turn on **Start with Windows** from the tray menu or *Settings > General*.

Windows may show a SmartScreen warning because the executable is not code-signed.

**macOS:** open the `.dmg`, drag `Retypr.app` to *Applications* and start it (first time: right-click > *Open*).
Retypr lives in the menu bar and needs two permissions in *System Settings > Privacy & Security*:
**Accessibility** and **Input Monitoring**. Grant them, then start Retypr again. The tray menu is the menu bar icon.

## Tray icon

| Icon | Meaning |
|---|---|
| Blue disc | Auto-switching is on. |
| Gray disc | Auto-switching is off, or Retypr is disabled for the focused app. |
| Disc split along the diagonal (blue upper-left, gray lower-right) | Auto-switching is on, but paused for the focused app (it is in the *No auto-switching* list). |

The tray menu: Settings, Auto-switching, Sounds, Start with Windows, Open settings folder, Restart as
administrator, Exit. Choosing *Settings* while the window is already open brings it to the front. *Exit* also closes
the settings window.

## Portable mode

Everything Retypr stores lives in a `data` folder **next to `Retypr.exe`**:

```
Retypr.exe
data/
  config.toml  created on first run
  dict/        word lists (en.txt, ru.txt, ...) and exceptions.txt
  sounds/      sound files (the release zip ships the default ones here)
```

* Move or copy the whole program folder anywhere - nothing breaks. Paths in `config.toml` may be **relative**; they
  are resolved against the `data` folder. Sound files you pick in the settings are copied into `data/sounds`
  so they stay portable.
  The default sounds are ordinary files, not part of the exe: replace `data/sounds/manual-switch.mp3` and
  `automatic-correction.mp3` to change them, or delete them for silence. If they are missing, Retypr just plays nothing.
* *Start with Windows* repoints itself to the new location the next time Retypr starts.
* If the program folder is not writable (for example *Program Files*), Retypr falls back to
  `%APPDATA%\retypr\config`. On macOS the app bundle is never written to: settings live in
  `~/Library/Application Support/retypr`. Set the `RETYPR_DATA_DIR` environment variable to force a folder.
* Settings from older versions (`%APPDATA%\retypr`) are not used any more.

## Hotkeys

Edit them in *Settings > Hotkeys* or in `config.toml`:

```toml
[hotkeys]
convert_word      = "Pause"
convert_selection = "Shift+Pause"
toggle_auto       = "Alt+Pause"
toggle_app_auto   = "Alt+Shift+Pause"
```

**macOS defaults** (Mac keyboards have no Pause key): `Ctrl+Alt+Z` convert the last word, `Ctrl+Alt+Shift+Z` convert the
selection, `Ctrl+Alt+A` toggle auto-switching, `Ctrl+Alt+Shift+A` toggle it for the current app. On a Mac `Alt` means
**Option** and `Win` means **Command** (`Option`, `Opt`, `Command` and `Cmd` are accepted too). Use `F13`-`F15` where
Windows uses `PrintScreen`, `ScrollLock` and `Pause`.

Format: modifiers (`Ctrl`, `Alt`, `Shift`, `Win`) plus one key: `Pause`, `Break` (= Ctrl+Pause), `ScrollLock`,
`CapsLock`, `Insert`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, `PrintScreen`, `Menu`, `Space`, `Tab`, `Enter`,
`Esc`, arrows, `F1`-`F24`, `A`-`Z`, `0`-`9`, `Grave`. For keyboards without a Pause key try
`Ctrl+Alt+Space`.

## Per-application rules

* **No auto-switching** (`auto_exclude_apps`) - the manual hotkeys still work. Defaults: VS Code, Visual Studio,
  JetBrains IDEs, Windows Terminal, cmd, PowerShell, PuTTY.
* **Retypr disabled** (`disabled_apps`) - all keys, including Pause, pass through untouched. Games, remote desktops.
* Press `Alt+Shift+Pause` in any window to toggle the first list for that app.

Names are executable names in lowercase: `code.exe` on Windows, `electron` or `terminal` on macOS (the name of the
binary inside the `.app`).

## Dictionaries and detection

Auto-switching considers every layout installed in Windows. For a finished word, Retypr tries each other layout:

1. Words with digits, mixed scripts, brackets or slashes (code, paths), acronyms (`USB`) and words in
   `dict/exceptions.txt` are left alone.
2. If the word is in the dictionary of its own language it is left alone. If its conversion is in the dictionary of
   another installed language, it is converted to that layout.
3. Words unknown to the dictionaries (names, slang) are judged by strict built-in rules (English, Russian and
   Ukrainian only) and only between layouts of different scripts. Other languages need a dictionary.

*Settings > Dictionaries* lists the installed layouts. For each one you can enable or disable it, and
**Download / Update** its word list (the 50,000 most frequent words from the
[FrequencyWords](https://github.com/hermitdave/FrequencyWords) project, CC-BY-SA 4.0). A running Retypr reloads
dictionaries automatically. Not every language has a list available.

## Configuration reference

`data/config.toml` - the settings window edits it, and a running Retypr applies changes within about a second.

| Key | Meaning |
|---|---|
| `auto_switch` | Automatic correction on/off. |
| `min_word_len` | Words shorter than this are never corrected automatically (default 2; two-letter words are converted only when a dictionary confirms it). |
| `disabled_languages` | Language codes (`"de"`, `"ru"`...) excluded from auto-switching. Empty = all installed layouts take part. |
| `auto_exclude_apps`, `disabled_apps` | Per-application rules (see above). |
| `select_after_convert` | `true`: select the converted text again after converting selected text, so the hotkey can be pressed once more to cycle layouts. Default `false`: the text is left unselected. |
| `caret_after_convert` | `"left"` or `"right"`: which end of the converted text the caret ends up at. |
| `selection_restore_limit` | Restoring the selection or caret after converting selected text costs one keystroke per character, so texts longer than this many characters are not fixed up (default 3000; 0 = no limit). |
| `theme` | Settings window theme: `"system"`, `"light"` or `"dark"`. |
| `dict_dir` | Optional dictionary folder (absolute or relative to `data`). Default `data/dict`. |
| `[sounds]` | `enabled`, `volume` (0.0-1.0, default 0.3) and `on_manual_switch`, `on_auto_switch`, `on_selection_convert`, `on_rule_toggle`: file paths, absolute or relative to `data`; empty = no sound. |
| `[hotkeys]` | See above. |

## Building

You need [Rust](https://rustup.rs). On Windows also the Visual Studio Build Tools with the *Desktop development with C++* workload.

| Platform | Command | Result |
|---|---|---|
| Windows | `build.cmd` | `dist\windows-x64\Retypr.exe` |
| macOS | `./build-macos.sh` | `dist/Retypr-<version>-macos-universal.dmg` |
| Linux | `./build.sh --install-deps` | `dist/linux-<arch>/Retypr` |

`build.cmd` options: `debug` (console window with logs), `run` (start after building), `rebuild` (clean rebuild of the
crate). The script refreshes file timestamps when sources changed, so Cargo never reuses a stale build.

macOS options and signing variables are described at the top of `build-macos.sh`.

Run the unit tests with `cargo test`.

The version lives in `Cargo.toml` (and in [CHANGELOG.md](CHANGELOG.md)); bump it for every release.

## Platform status

* **Windows** - the reference platform, full feature set.
* **macOS** - full feature set in principle (menu bar icon, settings window, any installed input source, per-app rules,
  dictionaries). New in 0.2.0 and developed without access to a Mac, so expect rough edges; please report them.
  Known differences: no code signing by default (see `build-macos.sh`), the *Restart as administrator* item does not
  exist, the focused app is detected by polling (up to ~0.25 s delay), and switching the input source depends on the
  system setting *Automatically switch to a document's input source*.
* **Linux** - X11 only, two layouts, no tray or settings window. Experimental.

## Troubleshooting

* **Hotkeys do nothing in some windows** - those windows run as administrator. Use the tray item
  *Restart as administrator*.
* **Auto-switching misbehaves in a program** - add its executable to the *No auto-switching* list.
* **The exe in `dist` is old** - run `build.cmd rebuild`.
* **macOS: nothing happens when typing** - the *Accessibility* and *Input Monitoring* permissions are missing (or were
  granted to an older copy). Remove Retypr from both lists, add it again, and restart it.
* **macOS: the app does not start from the DMG** - macOS blocks unsigned downloads: right-click > *Open* once, or run
  `xattr -dr com.apple.quarantine /Applications/Retypr.app`.

## Releases

GitHub Actions builds and publishes releases; see [RELEASING.md](RELEASING.md).

## Links

Source code: <https://github.com/DimmOFF/Retypr>
