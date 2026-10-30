# Tauri 2 with the scheduling and audio in Rust

Status: accepted

Tauri over Electron. An empty Tauri app installs at roughly 7-8 MB against
90-150 MB, and the parts that matter here are native anyway: the scheduler, the
audio engine, and the Windows calls that detect a locked screen and hand focus
back.

The webview is a rendering surface only. It draws the Card; it does not own time,
audio or OS state. That keeps the three platforms' webview differences (WebView2
on Windows, WebKitGTK on Linux, WKWebView on macOS) confined to CSS and layout,
where differences are visible and fixable, instead of to the product's behaviour.

_Consequences_: Linux builds and runs, but always-on-top over fullscreen apps is
not supported on Wayland; the overlay may sit under other windows there. Windows
is the supported target, macOS and Linux are best-effort.
