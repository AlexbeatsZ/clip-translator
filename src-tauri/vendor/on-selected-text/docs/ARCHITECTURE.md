# 架构维护说明

当前代码以 `AI_DOCS/selected-text-v3.md` 为目标蓝图，但采用渐进式落地策略：先建立稳定 API 与模块边界，再逐步替换平台内部实现，避免一次性引入大量不可验证的 unsafe / 系统 API 代码。

## 目录职责

```text
src/
├── lib.rs                  # 公开 API 与兼容入口
├── error.rs                # GetTextError 精确错误语义
├── options.rs              # GetTextOptions / DaemonOptions / PermissionStatus
├── result.rs               # SelectionResult / ScreenRect
├── provider.rs             # 非 macOS provider 链抽象
├── daemon/                 # Push 模式公共生命周期 + 三平台事件触发 + 轮询 fallback
│   ├── mod.rs              # DaemonHandle / start_selection_daemon / 触发器选择
│   ├── poll.rs             # 公共轮询 fallback 与选区去重
│   ├── macos.rs            # listen-only CGEventTap 鼠标触发
│   ├── windows.rs          # WH_MOUSE_LL 低级鼠标 Hook 触发
│   └── linux.rs            # AT-SPI2 ObjectEvents 触发
└── platform/
    ├── mod.rs              # cfg 平台分发
    ├── clipboard_guard.rs  # Linux/Windows 剪贴板事务保护
    ├── windows_clipboard.rs # Windows 原始格式快照 + 云剪贴板阻断
    ├── linux.rs            # Linux provider 入口
    ├── macos.rs            # macOS A11y provider 入口
    ├── macos_clipboard.rs  # macOS NSPasteboard 多 item 快照事务 + Cmd+C fallback
    └── windows.rs          # Windows provider 入口
```

## 当前策略链

```text
get_selected_text_with_options()
  -> platform::get_selected_text_with_options()
    -> macOS: AXSelectedTextRange + AXSelectedText + optional AXBoundsForRange -> AXManualAccessibility retry -> Rust NSPasteboard item transaction -> AppleScript Cmd+C only
    -> Linux: AT-SPI2 focused Text selection + optional bounds -> Wayland PRIMARY selection -> X11 PRIMARY selection -> ClipboardTransaction -> Enigo Ctrl+C -> arboard read/restore
    -> Windows: UIAutomation focused TextPattern2/TextPattern text + optional bounds -> raw format snapshot -> cloud/history blocking markers -> Enigo Ctrl+C -> arboard text read -> raw format restore
```

`ClipboardTransaction` 已完成三件事：

1. 用进程级 mutex 避免并发复制互相踩踏。
2. 进入事务时备份可恢复剪贴板内容，Drop 时尽量还原；macOS 使用 `pasteboardItems` 多 item 快照，Windows 使用 Win32 原始格式快照，Linux 仍使用 arboard 文本/图片快照。
3. 写入唯一 sentinel 并轮询变化，替代固定 `sleep(100ms)` 的旧判断方式。

## 与 v3 蓝图的对应关系

| v3 目标 | 当前状态 | 后续建议 |
| --- | --- | --- |
| typed API / options / result | 已落地 | 保持兼容，不再把新字段塞进旧入口 |
| ClipboardGuard + Mutex | Linux/Windows 已有兼容层；macOS 已使用 NSPasteboard 多 item 快照；Windows 已写入云剪贴板/历史阻断 markers 与原始格式快照 | 继续压低剩余 fallback 副作用 |
| macOS A11y 优先 | 已补 `AXSelectedTextRange` 联合判断、可选 `AXBoundsForRange` 坐标、`AXManualAccessibility` 最小重试与 Rust NSPasteboard item fallback 事务 | 后续补更完整的 Chromium 识别 |
| Linux PRIMARY / AT-SPI2 | AT-SPI2 focused Text selection 与 `fetch_bounds` range extents、Wayland PRIMARY、X11 PRIMARY、AT-SPI2 ObjectEvents Push 已接入 | 后续按真实桌面反馈细化 Push 防抖 |
| Windows UIAutomation | 已接入 focused TextPattern2 优先读取并回退 TextPattern；`fetch_bounds` 可读取第一段选区的 UIA bounding rectangles 并合并包围盒 | 后续继续按真实应用反馈补覆盖率 |
| bounds 坐标 | macOS Accessibility、Windows UIAutomation 与 Linux AT-SPI2 已接入；降级路径暂保持 `None` | 按真实应用反馈继续补覆盖率，失败时保持 `None` |
| Push daemon | 已接入公共 API、RAII `DaemonHandle`、macOS listen-only CGEventTap、Windows `WH_MOUSE_LL`、Linux AT-SPI2 ObjectEvents 与轮询 fallback | 后续按真实应用反馈细化触发策略，避免影响 Pull 模式稳定性 |

## 实施顺序建议

1. Push 细化：按真实桌面事件反馈收窄触发事件集合与防抖策略。
2. Windows / Linux / macOS 真实应用覆盖率：按 smoke 反馈继续补特例。
3. macOS Chromium 细化识别：继续减少 Electron/Chromium app 的 fallback 触发率。

## 回归检查

每次修改后至少运行：

```bash
cargo fmt
cargo test
cargo check --target x86_64-unknown-linux-gnu
cargo check --target x86_64-pc-windows-gnu
cargo clippy --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
```

如果本机未安装交叉目标，先执行：

```bash
rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-gnu
```
