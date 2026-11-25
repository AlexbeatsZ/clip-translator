# Architecture

## Chosen foundation

- Tauri 2 desktop shell.
- React + TypeScript for settings and result-panel UI.
- Rust core for input dispatch, provider clients, local image processing, and application state.
- Small platform adapters for macOS and Windows native APIs.

This keeps the shared product logic in one codebase while allowing native handling for selection access and floating-window behavior.

## Modules

- `input`: global shortcut, foreground app context, selected-text acquisition, clipboard snapshot/read/restore, payload classification.
- `ocr`: local OCR adapter returning text blocks, confidence, and bounding boxes.
- `translation`: provider trait plus DeepL and Google Cloud Translation implementations.
- `image_translation`: OCR block grouping, text-region cleanup, font selection, line wrapping, and compositing.
- `overlay`: native floating result panel: placement near cursor, content-fitted size, outside-click dismissal, pinning.
- `settings`: shortcut, language pair, active provider, and non-secret preferences (`settings.json` via tauri-plugin-store).
- `secrets`: OS credential vault integration for provider keys.

## Native platform behavior

### macOS

- Selection comes from `on-selected-text` (AX first, then an NSPasteboard-transaction copy fallback that snapshots and restores every pasteboard item). It is vendored under `src-tauri/vendor/on-selected-text` and patched via `[patch.crates-io]`: upstream's `active-win-pos-rs` → `appkit-nsworkspace-bindings` uses bindgen 0.68, whose build script segfaults with the Xcode 26 SDK. The patch reads the frontmost app name from `NSWorkspace` instead.
- Query selection through Accessibility APIs when available.
- Use a clipboard-preserving copy fallback only when required and permitted.
- Present results through an AppKit panel configured for the active full-screen Space (`fullScreenAuxiliary`) and non-activating behavior where possible. Outside clicks are detected with an `NSEvent` global mouse-down monitor.
- The app runs as a menu-bar utility: `Accessory` activation policy while settings are closed.
- Do not require Screen Recording permission for the initial clipboard-image workflow.

### Windows

- Query selected text through UI Automation TextPattern where supported.
- Use a clipboard-preserving copy fallback for applications that do not expose selection through UI Automation.
- Present results in a topmost tool window without taking focus unnecessarily; it hides on focus loss (after the user has clicked into it) unless pinned.

## OCR strategy

Decision: bundled cross-platform ONNX OCR — RapidOCR (`rapidocr-core`) with PaddleOCR PP-OCRv6 Small detection + recognition (no direction classifier), shipped in `src-tauri/resources/ocr-models/` and validated by `ModelCache` with downloads disabled. The engine is created lazily on the first image request and reused. The OCR interface stays independent of the translation provider.

## Translation providers

- DeepL API: user-supplied API key; support Free and Pro API endpoints.
- Google Cloud Translation: user-supplied API key and configured endpoint/version.
- Provider credentials are stored in Keychain on macOS and Credential Manager on Windows.
- Translation requests send text only. Images remain local unless the user explicitly selects a future cloud image/OCR provider.

## Image reconstruction

CPU only, off the UI thread (`spawn_blocking`). Per OCR block (adjacent lines are merged into paragraphs, `line_count` kept):

1. Sample a 2px ring around the padded box. If its luminance spread is small (flat UI background) fill the box with the ring's mean colour; otherwise add it to a mask for PatchMatch inpainting (`oximedia-restoration`). The flat fill path is ~100× faster and avoids smearing.
2. Estimate the source text colour from the pixels farthest from the background; fall back to dark/light by background luminance when contrast is too low.
3. Typeset with a system CJK font (PingFang / Microsoft YaHei …): start from the source line height, wrap by character (spaces preferred), shrink until it fits, allow 8% horizontal overflow, centre vertically.

If reconstruction fails the result falls back to text with a notice. Stale requests are dropped before translation/compose start.

## Reliability requirements

- Preserve all clipboard formats and restore them after any synthetic copy operation.
- Detect clipboard changes and avoid races with other clipboard managers.
- Bound OCR and translation request sizes; report unsupported or oversized images clearly.
- Cache model files locally and do not download models at runtime without explicit notice.
- Test multiple monitors, mixed DPI/scaling, full-screen apps, and keyboard focus behavior on both platforms.
