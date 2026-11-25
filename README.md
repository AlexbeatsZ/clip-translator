# ClipLingo

A clipboard-first desktop translator for macOS and Windows. One global shortcut resolves the current text selection or clipboard payload, then translates it with a configurable traditional translation provider.

## Product principles

- One configurable global shortcut; no separate text/OCR/image modes.
- Selected text takes priority. Otherwise, clipboard images go through local OCR and local image reconstruction; clipboard text is translated directly.
- OCR and image processing stay on-device. Only extracted text is sent to a remote translation provider.
- macOS and Windows share the Rust core while using native adapters for selection access and full-screen-safe floating panels.

## Development

```sh
pnpm install
pnpm tauri dev
pnpm build
```

See `docs/design/product.md` and `docs/design/architecture.md` for the current specification.
