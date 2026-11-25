# Vendored copy of on-selected-text 0.1.6

Upstream: https://github.com/uxiew/on-selected-text (MIT OR Apache-2.0, by yetone and contributors).

Local patch, applied through `[patch.crates-io]` in `src-tauri/Cargo.toml`:

- macOS `active_app_name()` uses `objc2-app-kit` `NSWorkspace.frontmostApplication` instead of
  `active-win-pos-rs`. The latter depends on `appkit-nsworkspace-bindings`, whose bindgen 0.68
  build script segfaults against the Xcode 26 / clang 21 macOS SDK.

Remove this vendor copy once upstream drops the `active-win-pos-rs` dependency.
