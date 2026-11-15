Retypr - keyboard layout switcher
=================================

INSTALL
  1. Drag Retypr.app onto the Applications folder.
  2. First launch: right-click Retypr.app -> Open -> Open.
     (If macOS still refuses: run in Terminal
      xattr -dr com.apple.quarantine /Applications/Retypr.app )

PERMISSIONS (required, otherwise the keyboard hook receives nothing)
  System Settings -> Privacy & Security ->
    * Accessibility      -> add Retypr and switch it on
    * Input Monitoring   -> add Retypr and switch it on
  Then quit and start Retypr again.

QUIT
  Retypr runs in the background without a Dock icon.
  Quit it with:  pkill retypr    (or Activity Monitor -> Retypr -> Quit)

CONFIG
  ~/Library/Application Support/retypr/config.toml
  Dictionaries (better automatic detection): run get-dicts.sh from this disk image
  (Terminal: bash "/Volumes/Retypr/get-dicts.sh").

KNOWN LIMITATIONS OF THE macOS BUILD
  The macOS port is still early: there is no menu bar icon or settings window yet
  (edit config.toml by hand), and switching the system input source is not implemented
  yet - text is converted, but you have to switch the layout yourself.
