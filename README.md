# Wali

Wali is an unofficial Linux desktop client for WhatsApp Web, built with Tauri 2 and the system WebKitGTK runtime. It aims to provide a lightweight, Linux-native desktop experience without modifying WhatsApp's UI.

## Features

- Persistent WhatsApp Web session
- Native Linux / Wayland window
- System tray (Show Wali, Settings, Quit)
- Close-to-tray
- Single-instance behavior
- Native desktop notifications
- Microphone and camera permissions for supported WhatsApp features
- Downloads to the system Downloads directory
- External links opened in the default browser
- Local Settings window
- Strict separation between WhatsApp Web and Tauri IPC

## Current limitations

- WhatsApp voice and video calling is currently unsupported.
- Wali is currently Linux-focused.
- The project is in early development.

## Technology

- Rust
- Tauri 2
- System WebKitGTK 4.1
- Plain HTML, CSS, and JavaScript for Settings

WhatsApp itself is loaded from `https://web.whatsapp.com`. Wali does not inject JavaScript or CSS into that page and does not implement a custom WhatsApp protocol.

## Requirements

Linux, plus a Rust toolchain and the native libraries needed to build a GTK 3 / WebKitGTK 4.1 application:

- Rust toolchain (`rustc`, `cargo`)
- GTK 3 development libraries
- WebKitGTK 4.1 development libraries
- pkg-config
- A standard Linux desktop (tray, notifications, default browser)

On Arch Linux / CachyOS, the packages used while developing Wali were:

```bash
sudo pacman -S rust gtk3 webkit2gtk-4.1 pkgconf
```

That list is an example from one machine, not a support matrix.

## Build

From the repository root:

```bash
cd src-tauri
cargo build --release
```

The binary is written to `src-tauri/target/release/wali`.

```bash
./src-tauri/target/release/wali
```

First launch opens WhatsApp Web. Scan the QR code to sign in. The session is stored locally under the application id `io.github.mo999salah.wali`.

## Security / Privacy

WhatsApp Web is treated as remote, untrusted content. The main WhatsApp WebView is not given Wali/Tauri command capabilities. Local Settings IPC is restricted to the Settings window. Wali does not include analytics or telemetry.

## Disclaimer

Wali is an unofficial WhatsApp client. It is not affiliated with, endorsed by, or associated with WhatsApp or Meta.

"WhatsApp" and related trademarks belong to their respective owners.

## License

MIT License. See [LICENSE](LICENSE).
