# Product brief

## Positioning

A small desktop utility that translates whatever is currently selected or copied. One configurable global shortcut is the primary interaction. No separate screenshot-capture workflow is required in the MVP: users can use the operating system's screenshot-to-clipboard shortcut and then invoke the translator.

## Supported platforms

- macOS 13.5+
- Windows 10+

## Primary interaction

When the global shortcut is pressed, resolve input in this order:

1. **Selected text** in the foreground application: translate directly.
2. **Clipboard image**: run local OCR, translate recognized text, then show a translated image with the original text visually replaced.
3. **Clipboard text**: translate directly.
4. **No usable input**: show a concise explanation without changing clipboard contents.

Selection detection must use native accessibility/text-selection APIs where possible. A copy-based fallback may be used only if the clipboard can be fully snapshotted and restored, including image formats. Do not silently replace the user's clipboard.

If the clipboard exposes both image and text representations, selected text takes priority; otherwise prefer the image for screenshot workflows.

## Translation behavior

- Default target language: Simplified Chinese.
- Source language: automatic detection by default, with optional manual override.
- MVP providers: DeepL API and Google Cloud Translation.
- Provider choice is configurable; credentials belong in the operating system credential vault, not plaintext settings files.
- Keep provider implementations behind one interface so additional traditional translation services can be added without changing the UI or input pipeline.

## Image translation

The default image mode is **in-place visual replacement**:

1. OCR returns text, confidence, and bounding boxes.
2. Group OCR fragments into lines/paragraphs before translation.
3. Translate grouped text while preserving reading order.
4. Remove or cover the original text locally.
5. Typeset translated text into the original regions, adapting font size and line breaks.
6. Display the translated image in a lightweight overlay/panel. Keep the original image available for comparison and copying.

Use ordinary image processing/inpainting for the first version. Do not require a generative image model. Complex textured backgrounds may need a later, optional enhancement path.

## UX constraints

- Exactly one primary global shortcut; no separate shortcuts for text, OCR, and image translation.
- A compact result panel with source text, translated text, copy, and optional pronunciation.
- Image results should support viewing the original and translated image, copying, and saving.
- No always-on clipboard monitoring in the MVP. Read clipboard only after the user invokes the shortcut.
- Do not steal focus from the foreground application unless the user explicitly interacts with the panel.
- On macOS, the result panel must remain in the current full-screen Space rather than forcing a switch to the desktop.

## Out of scope for MVP

- Built-in screenshot selection UI.
- Continuous clipboard monitoring.
- AI/LLM translation as the default path.
- Cloud OCR as a requirement.
- Account/subscription system or a hosted backend.
