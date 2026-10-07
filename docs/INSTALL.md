# Installing the choscorui demo apps

This archive contains three macOS apps built with choscorui:

| App | What it shows |
| --- | --- |
| choscorui Gallery | Basic controls, text editing with input methods, multilingual text, theme switching |
| choscorui Stress | 100,000 searchable records, a 20,000-item canvas, docking, settings restored across launches |
| choscorui Notes | A small multi-window app in English, Vietnamese, and Hebrew (right to left) |

## Requirements

macOS 15 or later, on Apple silicon or Intel (an Intel build needs the universal archive).

## Install

```sh
./install.sh                                    # into ~/Applications
CHOSCORUI_INSTALL_DIR=/Applications ./install.sh # for all users (may need sudo)
```

The script checks the macOS version and each app's code signature, then copies the apps. Open them from Finder or Launchpad.

If the archive is not notarized (development builds are signed ad hoc), macOS blocks the first launch. To open the app anyway, Control-click it in Finder and choose **Open**. Release archives are notarized and open normally.

## Check the core scenarios

Each app has an unattended check that exits non-zero on failure:

```sh
"$HOME/Applications/choscorui Gallery.app/Contents/MacOS/choscorui-gallery"  # with CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000
CHOSCORUI_STRESS_AUTOPILOT=1 "$HOME/Applications/choscorui Stress.app/Contents/MacOS/choscorui-stress"
NOTES_AUTOPILOT=1 "$HOME/Applications/choscorui Notes.app/Contents/MacOS/choscorui-notes-sample"
```

## Uninstall

```sh
./uninstall.sh              # removes the apps and their settings, preferences, caches, and saved window state
./uninstall.sh --keep-data  # removes only the apps
```

The script removes nothing else. The apps write only to these locations, keyed by their bundle identifiers (`com.choscor.gallery`, `com.choscor.stress`, `com.choscor.notes`):
- `~/Library/Application Support/<id>`
- `~/Library/Preferences/<id>.plist`
- `~/Library/Caches/<id>`
- `~/Library/Saved Application State/<id>.savedState`
