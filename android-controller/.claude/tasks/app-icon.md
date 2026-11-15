# Task — new launcher icon from music-controller-icon.svg (android-controller)
Follow the **Ownership** rules in [_workflow-rules.md](_workflow-rules.md). The five MVP steps don't apply; follow the steps below.
Run it by asking this window: "follow `.claude/tasks/app-icon.md`".

## Goal
The app's launcher icon (home screen, app drawer, settings, notification large icon if it uses the launcher icon) becomes `music-controller-icon.svg` (in this folder): a dark rounded tile, a white knob in a grey ring, and five equalizer bars. The app label "AM Remote" stays.

## Rules
- **Don't install anything system-wide.** A Dart dev-dependency (`flutter_launcher_icons`) is fine, because `flutter pub get` handles it. If a step needs a tool that isn't there, stop and tell the user what to install.
- **No C2PA metadata in the app.** The SVG ends with a large `<metadata><c2pa:manifest>…` block. Copies made for the app drop it; leave the original file as it is.
- Don't hand-edit generated `mipmap-*` files afterwards; regenerate instead.

## Steps
1. **Sources.** Create `assets/icon/`:
   - `icon.svg`: the SVG without the `<metadata>` block (the full icon, tile included).
   - `foreground.svg`: the same art **without the tile** (drop the `<g clip-path="url(#tile)">` group), with everything else wrapped in `<g transform="translate(512 512) scale(0.78) translate(-512 -540)">` so the ring, knob and bars sit inside the adaptive-icon safe zone (the centre 66 %, a circle of ~676 px in 1024). Check the bars and the ring are not cut by a round mask.
2. **Render to PNG (1024 × 1024).** `flutter_launcher_icons` needs PNG. Use Microsoft Edge, which every Windows has, in headless mode (no install):
   ```powershell
   $edge = "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
   & $edge --headless --disable-gpu --hide-scrollbars --default-background-color=00000000 --window-size=1024,1024 --screenshot="$PWD\assets\icon\icon.png" "file:///$($PWD -replace '\\','/')/assets/icon/icon.svg"
   & $edge --headless --disable-gpu --hide-scrollbars --default-background-color=00000000 --window-size=1024,1024 --screenshot="$PWD\assets\icon\foreground.png" "file:///$($PWD -replace '\\','/')/assets/icon/foreground.svg"
   ```
   If Edge is in `Program Files` instead, adjust the path. Open both PNGs (Read tool) and check: 1024 × 1024, transparent corners, nothing cropped, the drop shadow visible. If rendering fails, ask the user to export the two PNGs with any tool (Inkscape, Figma, a browser) and continue.
3. **Generate.** Add `flutter_launcher_icons` to `dev_dependencies` and its config in `pubspec.yaml`:
   ```yaml
   flutter_launcher_icons:
     android: true
     ios: false
     image_path: "assets/icon/icon.png"
     adaptive_icon_background: "#161618"
     adaptive_icon_foreground: "assets/icon/foreground.png"
     min_sdk_android: 24
   ```
   Then `flutter pub get` and `dart run flutter_launcher_icons`. It rewrites `android/app/src/main/res/mipmap-*` and adds `mipmap-anydpi-v26/ic_launcher.xml`. The manifest keeps `@mipmap/ic_launcher`.
4. **Notification icon.** If `audio_service`'s notification uses a small icon (`androidNotificationIcon`), leave it: Android needs a white-on-transparent small icon, not this one. Note in the report if it currently points at `ic_launcher`.
5. **Check** with `/verify`, then `flutter build apk --release` and install it.

## Done when
- `/verify` is green, and the release APK shows the new icon on the phone's home screen and app drawer, both round and squircle shapes (adaptive), without cut-off bars.
- `assets/icon/` has the two SVGs (without metadata) and the two PNGs, and `music-controller-icon.svg` stays where it is: the root `README.md` shows it as the project logo.

## Smoke test (→ docs/smoke-tests/app-icon.md)
1. Uninstall the old app, install the new APK → the new icon shows in the app drawer.
2. Long-press the home screen → change the icon shape (if the launcher allows) → the art stays inside the shape.
3. Settings → Apps → AM Remote → the same icon.

## Report
What was generated, the files changed, how it looks (shape check), and a commit suggestion: `feat(app): new launcher icon`.
