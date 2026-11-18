Retypr - keyboard layout switcher
=================================

INSTALL
  1. Drag Retypr.app onto the Applications folder.
  2. First launch: right-click Retypr.app -> Open -> Open.
     (If macOS still refuses, run in Terminal:
      xattr -dr com.apple.quarantine /Applications/Retypr.app )

PERMISSIONS (required, otherwise the keyboard tap receives nothing)
  System Settings -> Privacy & Security ->
    * Accessibility      -> add Retypr and switch it on
    * Input Monitoring   -> add Retypr and switch it on
  Then quit and start Retypr again.

USE
  Retypr lives in the menu bar (no Dock icon). Click its icon for the menu:
  Settings, Auto-switching, Sounds, Start at login, Open settings folder, Quit Retypr.

  Default hotkeys (change them in Settings > Hotkeys):
    Ctrl+Option+Z            convert the last word (press again to cycle through all layouts)
    Ctrl+Option+Shift+Z      convert the selected text
    Ctrl+Option+A            turn auto-switching on or off
    Ctrl+Option+Shift+A      turn auto-switching on or off for the current app

  Automatic correction needs word lists: Settings > Dictionaries > Download.
  The menu bar icon is blue when auto-switching is on, gray when it is off, and half blue / half gray when it is
  paused for the app you are using.

FILES
  ~/Library/Application Support/retypr/    config.toml, dict/ (word lists), sounds/
  The default sounds are copied there from the app bundle on first run.

NOTE
  This is the first macOS release; it was written without access to a Mac, so please report any problem
  at https://github.com/DimmOFF/Retypr
