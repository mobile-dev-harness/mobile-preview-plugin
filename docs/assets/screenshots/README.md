# Public screenshots

Captured on 2026-10-10 (Asia/Shanghai) in stock DSH Web `0.2.1-alpha.1`, using the
published MPP `v0.1.0-preview.5` archive on Apple Silicon macOS.
The installed runtime manifest identifies source commit
`31a2bdb1c4d96345c7735377d6d861fe52ce12a4`.

- `dsh-ios-workflow.jpg`: iPhone 17 / iOS 26.4 / Xcode 26.4; live system Settings.
- `dsh-android-workflow.jpg`: Android 15 / API 35 / arm64 emulator; live system Settings.

The real chat requests an existing Android 15 emulator or an iOS 26.4 iPhone 17
Simulator and its preview panel. The agent checks the local environment and opens
the platform panel through `open_mobile_preview`; the user selects the device and
connects. The connected devices display their system Settings apps. Both images are
1000 × 586 viewport crops that omit the local host header and workspace sidebar.
No device frames, chat text or controls were composited or generated.

These images illustrate the listed environments. They do not extend the formal
[release qualification](../../RELEASING.md). The website hosts identical copies.
