# ChoscorUI gallery

This archive contains `choscorui Gallery`, a macOS app that shows each ChoscorUI control on its own page. The app has a page filter, three themes, English and Vietnamese text, and many windows. It keeps its settings between starts.

## Requirements

You need macOS 15 or later. On an Intel Mac, use the universal archive.

## Install the app

1. Open Terminal in the folder of this file.
2. Run `./install.sh`. The script copies the app into `~/Applications`.
3. Open the app from Finder or Launchpad.

To install for all users, run `CHOSCORUI_INSTALL_DIR=/Applications ./install.sh`. This command can need `sudo`.

The script checks the macOS version and the code signature of the app before it copies the app.

A development archive has an ad hoc signature, and macOS stops the first start of its app. To open the app, Control-click it in Finder and select **Open**. A release archive is notarized, and its app opens normally.

## Check the app

The second command runs the automatic checks. Each command exits with a status that is not zero when a check fails.

```sh
CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 "$HOME/Applications/choscorui Gallery.app/Contents/MacOS/choscorui-gallery"
CHOSCORUI_GALLERY_AUTOPILOT=1 "$HOME/Applications/choscorui Gallery.app/Contents/MacOS/choscorui-gallery"
```

## Remove the app

1. Open Terminal in the folder of this file.
2. Run `./uninstall.sh`. The script removes the app and its data.

To keep the settings and preferences, run `./uninstall.sh --keep-data`. The script then removes only the app.

The app writes data only to these locations. The `<id>` is the bundle identifier `com.choscor.gallery`.

- `~/Library/Application Support/<id>`
- `~/Library/Preferences/<id>.plist`
- `~/Library/Caches/<id>`
- `~/Library/HTTPStorages/<id>`
- `~/Library/Saved Application State/<id>.savedState`

## Licenses

The app is available under the MIT license or the Apache License 2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`. `THIRD-PARTY-NOTICES.txt` contains the notices for third-party code in the app.
