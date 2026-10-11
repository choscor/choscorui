# choscorui demo apps

This archive contains three macOS apps that use choscorui.

| App | Content |
| --- | --- |
| choscorui Gallery | Basic controls, text input with input methods, multilingual text, and theme switching |
| choscorui Stress | A list of 100,000 searchable records, a canvas with 20,000 items, docking panels, and saved settings |
| choscorui Notes | A small app with many windows in English, Vietnamese, and Hebrew (right to left) |

## Requirements

You need macOS 15 or later. On an Intel Mac, use the universal archive.

## Install the apps

1. Open Terminal in the folder of this file.
2. Run `./install.sh`. The script copies the apps into `~/Applications`.
3. Open the apps from Finder or Launchpad.

To install for all users, run `CHOSCORUI_INSTALL_DIR=/Applications ./install.sh`. This command can need `sudo`.

The script checks the macOS version and the code signature of each app before it copies the apps.

A development archive has an ad hoc signature, and macOS stops the first start of its apps. To open such an app, Control-click it in Finder and select **Open**. A release archive is notarized, and its apps open normally.

## Check the apps

Each command does an automatic check of one app. The command exits with a status that is not zero when the check fails.

```sh
CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 "$HOME/Applications/choscorui Gallery.app/Contents/MacOS/choscorui-gallery"
CHOSCORUI_STRESS_AUTOPILOT=1 "$HOME/Applications/choscorui Stress.app/Contents/MacOS/choscorui-stress"
NOTES_AUTOPILOT=1 "$HOME/Applications/choscorui Notes.app/Contents/MacOS/choscorui-notes-sample"
```

## Remove the apps

1. Open Terminal in the folder of this file.
2. Run `./uninstall.sh`. The script removes the apps and their data.

To keep the settings and preferences, run `./uninstall.sh --keep-data`. The script then removes only the apps.

The apps write data only to these locations. Each `<id>` is a bundle identifier: `com.choscor.gallery`, `com.choscor.stress`, or `com.choscor.notes`.

- `~/Library/Application Support/<id>`
- `~/Library/Preferences/<id>.plist`
- `~/Library/Caches/<id>`
- `~/Library/HTTPStorages/<id>`
- `~/Library/Saved Application State/<id>.savedState`

## Licenses

The apps are available under the MIT license or the Apache License 2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`. `THIRD-PARTY-NOTICES.txt` contains the notices for third-party code in the apps.
