# UI / UX

Read this before changing `src/` (settings window, result panel) or `src-tauri/src/overlay.rs`.

## Principles

- Native-feeling utility, not a landing page: system font, 13px base, grouped rows like macOS System Settings, light/dark from `prefers-color-scheme`. No marketing copy, no decorative illustrations.
- The app lives in the menu bar / tray. Closing the settings window hides it; macOS switches to `Accessory` activation policy (no Dock icon) while settings are closed and back to `Regular` while open.
- First launch (no API key stored) opens settings; later launches start silently. A second launch focuses the existing instance (single-instance plugin).

## Settings window (`src/settings/`)

- Sidebar: 通用 / 翻译服务 / 图片翻译 / 关于. A dot marks 翻译服务 when the active provider has no key.
- 通用 starts with a readiness card: either "已就绪" (with a "翻译剪贴板" test button) or a to-do list (missing key, missing Accessibility permission) with one action each.
- Shortcut recorder uses `KeyboardEvent.code` (physical key), never `event.key` — ⌥T yields "†" on macOS. It suspends the registered global hotkey while recording so pressing the current combination does not start a translation. A shortcut needs ⌘/⌃/⌥ (or Ctrl/Alt/Win), except F-keys.
- Provider key: password field → save to OS vault → automatic test request ("Hello, world!") with inline result. Configured keys show 测试连接 / 更换 / 移除 (two-step confirm). Keys are never read back into the UI.
- Preferences save immediately on change; there is no Save button.

## Result panel (`src/result/ResultPanel.tsx`, `src-tauri/src/overlay.rs`)

- Opens next to the mouse cursor on the cursor's monitor, flips above the cursor near the bottom edge, and is clamped to the work area.
- Sizes itself to content: the web view measures its natural height and calls `fit_result_panel`; the backend keeps the top-left corner and re-clamps to the screen. Text results are 420pt wide; image results follow the image's point width (400–720pt).
- Never takes focus when shown (macOS non-activating `NSPanel`, joins full-screen Spaces). Closes on Esc (once focused), the × button, or any click in another app (global mouse-down monitor; does not need Accessibility). The pin button keeps it open.
- States: loading (stage text + image pipeline progress track + skeleton), text result, image result, error. Each request carries a `requestId`; stale progress/results are ignored on both sides.
- Text result: source (clamped to 4 lines, expandable) → language route with a target-language switch that re-translates in place → translation (15px) → footer with provider name, 复制原文 (ghost) and 复制译文 (primary).
- Image result: segmented 译图 / 原图 / 文字; actions 复制译图, 保存 (to Downloads, with 显示 link). If translation fails the OCR text is still viewable and copyable. If reconstruction fails the result falls back to text with a notice.
- Errors carry an `errorCode`: `missing-key` offers "配置 … 密钥" (opens settings), `no-input` explains what to select/copy, others offer 重试.

## Previewing the UI without the desktop shell

`pnpm dev` and open `http://localhost:1420/` in a browser: `src/lib/mock.ts` stands in for the backend (dev builds only).

- Settings: `?configured=0` (no key), `&ax=denied`, `&platform=windows`.
- Panel: `?view=result&mock=text|long|image|ocr|loading|missing-key|no-input`.

Dev builds also forward web-view errors and a boot line to the `tauri dev` terminal as `[webview] …` (see `src/main.tsx`, `vite.config.ts`).
