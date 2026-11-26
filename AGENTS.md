# Goal

Build a cross-platform desktop translator (ClipLingo) for macOS and Windows. A single global shortcut resolves the current selection or clipboard payload and translates it with traditional translation providers (DeepL, Google Cloud). OCR and image reconstruction run locally; only text is sent to the selected provider.

# Current State

- Version 0.2.0, Tauri 2 + React 19 + TypeScript, Rust core. Repo: `AlexbeatsZ/clip-translator` (private).
- Implemented and verified on macOS (Apple Silicon, Xcode 26):
  - Menu-bar app (tray menu: 翻译 / 设置 / 退出), single instance, first run opens settings.
  - Global shortcut (default ⌥⇧T) → selection (AX, clipboard-preserving fallback) → clipboard image → clipboard text.
  - Floating result panel: near cursor, fits content, non-activating, closes on outside click/Esc, pin, stage progress, retranslate to another target language, copy/save.
  - Local OCR (RapidOCR + PP-OCRv6 Small) and image reconstruction (flat fill or PatchMatch inpaint, colour-matched CJK typesetting).
  - DeepL (Free/Pro auto endpoint) and Google Cloud Translation; keys in Keychain / Credential Manager; connection test.
  - Keyless Yandex provider (unofficial mobile endpoint; needs a mobile User-Agent or it returns 403).
  - Settings UI rewritten from scratch (see `docs/design/ui.md`).
- Release v0.2.0 (signed .app/.dmg, arm64) is on GitHub Releases.
- Not yet verified: physical ⌥⇧T press on the release build, real DeepL/Google requests (no API key on the dev machine), Windows build/behaviour, multi-monitor/mixed DPI placement, outside-click dismissal in a real full-screen app.

# Active Work

- End-to-end test with a real DeepL or Google key (text + image with actual translations).
- Validate on Windows (OMEN / ROG): UIA selection, panel focus/hide behaviour, tray icon, NSIS bundle.
- Test the panel in a full-screen app and across displays with different scale factors.
- Optional: autostart at login, translation history.

# Build / Run / Test

- `pnpm install` (lockfile: pnpm-lock.yaml).
- UI preview without the desktop shell: `pnpm dev`, open `http://localhost:1420/?…` (mock scenarios in `docs/design/ui.md`).
- Desktop dev: `pnpm tauri dev`. Web-view errors show up in that terminal as `[webview] …`.
- Checks: `pnpm build` and `cd src-tauri && cargo fmt --check && cargo test`.
- Local-model tests: `cargo test --lib -- --ignored` (OCR, compose). Visual pipeline check: `CLIPLINGO_SAMPLE=in.png CLIPLINGO_OUT=out.png cargo test --lib -- --ignored ocr_and_compose --nocapture`.
- Release (macOS): `pnpm tauri build` — signed with the fixed `Local Development Code Signing` identity (`bundle.macOS.signingIdentity`); verify with `codesign -d -r- <app>` (must contain `certificate leaf`).
- Windows packaging must be validated on Windows.

# Durable Lessons

- Do not assume clipboard text is the current selection. Query the focused application's selection first; any copy-based fallback must snapshot and restore all clipboard formats.
- The macOS result panel must stay a non-activating panel on the active full-screen Space. Test in a real full-screen app.
- `on-selected-text` is vendored and patched (`src-tauri/vendor/`) because `appkit-nsworkspace-bindings` (bindgen 0.68) segfaults on the Xcode 26 SDK. Setting `LIBCLANG_PATH` does not help.
- Every Tauri window that calls `listen`/window APIs must be listed in `src-tauri/capabilities/default.json`; the old panel was missing and silently never received results.
- WKWebView does not paint occluded windows, so `screencapture -l` of a background Tauri window is blank even when the UI rendered. Bring it forward (e.g. launch the binary again; single-instance focuses it) before capturing.
- The dev binary has no bundle id, so computer-use cannot target it; list its windows with `CGWindowListCopyWindowInfo` and capture with `screencapture -l <id>`.
- Release links ONNX Runtime's prebuilt static lib, which needs compiler-rt (`___isPlatformVersionAtLeast`); `src-tauri/build.rs` links `libclang_rt.osx.a` from `xcrun clang --print-resource-dir`. Debug builds hid this.
- `clip-translator --translate` on a running instance triggers one translation (single-instance args); use it for automation/tests instead of synthetic CGEvent hotkeys, which proved unreliable.
- The interactive shell has `noclobber`; use `>|` when re-creating log files.

- Google free `client=gtx` endpoint works from curl but reqwest gets a 429 "Sorry" bot-check page; do not try to evade it.

# Docs Index

- `docs/design/product.md`: user-facing behavior and MVP scope.
- `docs/design/architecture.md`: modules, native integrations, OCR and image reconstruction, provider boundaries.
- `docs/design/ui.md`: settings window and result panel behaviour, mock preview.
- `docs/log/2026-11.md`: dated implementation decisions and validation.
