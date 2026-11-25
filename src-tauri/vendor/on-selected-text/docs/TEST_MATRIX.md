# 真实测试矩阵

本文档用于规划 `on-selected-text` 的真实桌面 smoke 验证。单元测试只能覆盖纯逻辑和编译契约，选区读取还必须在真实 GUI、权限和应用组合中验证。

## Smoke 工具

```bash
cargo run --example pull_smoke -- --bounds
cargo run --example daemon_smoke -- --bounds
cargo run --example pull_smoke -- --no-clipboard-fallback
cargo run --example daemon_smoke -- --trigger-delay-ms 50 --drag-threshold-px 4
cd bindings/node && npm run build && npm run smoke
cd bindings/node && npm test
cd bindings/node && npm run test:binding
cd bindings/node && npm run pack:dry-run
cd bindings/node && npm run verify:platform-packages
cd bindings/node && npm run smoke:interactive
cd bindings/node && npm run smoke:interactive -- --label textedit --expect-contains "expected text"
```

记录每次测试时请包含：操作系统版本、桌面环境、应用名称与版本、是否开启坐标、是否允许剪贴板 fallback、实际命中的行为、失败错误文本。

- Node 交互式 smoke：运行 `npm run smoke:interactive` 后，在倒计时内切到目标应用并保持文本选中；若偶发 `AX focused element` 或 pasteboard marker 失败，脚本会短暂 settle 并有限重试，仍失败才记录为真实失败。
- 可断言 Node smoke：追加 `--expect-text`、`--expect-contains`、`--expect-regex` 或 `--min-length`，脚本会输出 `assertion` JSON；建议真实应用回归都写入期望，避免只靠肉眼判断。
- 独立 GUI consumer 回归：连续划词 30 次，观察弹窗/工具条生命周期、daemon 是否持续运行，并在 Activity Monitor 或系统任务管理器中确认宿主进程 CPU 没有持续高占用。

## macOS

| 场景 | Pull | Push | bounds | 重点观察 |
| --- | --- | --- | --- | --- |
| TextEdit 普通文本 | 必测 | 必测 | 必测 | Accessibility 直读与 CGEventTap 触发 |
| Safari 网页文本 | 必测 | 必测 | 建议 | Web 内容 AX 映射与坐标稳定性 |
| Chrome / Chromium | 必测 | 必测 | 建议 | `AXManualAccessibility` 重试是否减少 fallback |
| VS Code / Electron | 必测 | 必测 | 建议 | Electron 选区、fallback 副作用 |
| WeChat / 微信聊天文本 | 必测 | 建议 | 可选 | 自绘 UI，可能需要 A11y focused element 重试与剪贴板 fallback |
| Finder 文件选择 | 必测 | 可选 | 不适用 | Cmd+C fallback 应返回文件路径文本，且恢复剪贴板 |
| 富文本 / HTML 内容 | 建议 | 可选 | 可选 | 剪贴板 fallback 应从 RTF/HTML 提取纯文本表示 |
| Terminal | 建议 | 建议 | 可选 | 终端文本选择与权限提示 |
| Accessibility 未授权 | 必测 | 可选 | 可选 | `check_permissions()` 与失败提示 |

## Windows

| 场景 | Pull | Push | bounds | 重点观察 |
| --- | --- | --- | --- | --- |
| Notepad / 新版记事本 | 必测 | 必测 | 必测 | UIAutomation TextPattern2 / TextPattern |
| Chrome / Edge | 必测 | 必测 | 建议 | Web 文本与 `WH_MOUSE_LL` 触发 |
| VS Code / Electron | 必测 | 必测 | 建议 | UIAutomation 覆盖率与 fallback |
| Office / WordPad 类富文本 | 建议 | 建议 | 建议 | 多段选区与 bounding rectangles |
| 剪贴板历史开启 | 必测 | 可选 | 可选 | fallback 写入是否阻断历史 / 云同步 |
| UIAutomation 不支持应用 | 建议 | 建议 | 可选 | raw clipboard restore 是否正常 |

## Linux

| 场景 | Pull | Push | bounds | 重点观察 |
| --- | --- | --- | --- | --- |
| GNOME Wayland + GTK 文本框 | 必测 | 必测 | 必测 | AT-SPI2 直读与 ObjectEvents |
| GNOME X11 + GTK 文本框 | 必测 | 必测 | 必测 | AT-SPI2 与 X11 PRIMARY 顺序 |
| Firefox / Chrome | 必测 | 必测 | 建议 | 浏览器无障碍树和 PRIMARY 行为 |
| VS Code / Electron | 必测 | 必测 | 建议 | Electron AT-SPI2 覆盖率 |
| Terminal | 建议 | 建议 | 可选 | PRIMARY selection 是否优先 |
| 无 AT-SPI2 session | 建议 | 可选 | 可选 | Wayland / X11 / clipboard fallback |

## 通过标准

- Pull 模式：有选区时返回非空文本；无选区时返回 `NoTextSelected` 或旧入口空字符串。
- Push 模式：拖拽选择、双击选词、三击选段应触发一次或少量去重后的回调，不应持续热循环。
- bounds：开启 `--bounds` 后，支持的平台/应用返回合理矩形；不支持时允许 `None`，但不能影响文本读取。
- fallback：允许 fallback 时应尽量恢复原剪贴板；禁止 fallback 时不应模拟复制。
- 权限：权限缺失应能被 `check_permissions()` 或 `PermissionDenied` 暴露，不能静默表现为成功。

## 回归记录模板

```text
Date:
OS / desktop:
App / version:
Command:
Selected text source:
Expected:
Actual:
Bounds:
Fallback allowed:
Clipboard restored:
Notes:
```
