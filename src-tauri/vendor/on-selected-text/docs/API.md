# API 维护说明

本文档描述当前 crate 的公开 API、错误语义与迁移约定，方便维护者在后续实现 `AI_DOCS/selected-text-v3.md` 中的工业级能力时保持兼容。

## 公开入口

### `get_selected_text()`

```rust
pub fn get_selected_text() -> Result<String, Box<dyn std::error::Error>>
```

兼容旧版本的便捷入口：

- 成功时返回选中文本。
- 当前没有选中文本时返回 `Ok(String::new())`，保留旧行为。
- 权限缺失、平台失败等仍返回 `Err`。

新代码如果需要精确区分“无选中文本”和“提取失败”，应使用 `get_selected_text_with_options()`。

### `get_selected_text_with_options()`

```rust
pub fn get_selected_text_with_options(
    opts: GetTextOptions,
) -> Result<SelectionResult, GetTextError>
```

面向 v3 架构的 typed API：

- `SelectionResult.text`：选中文本；macOS 剪贴板 fallback 会按 best-effort 从纯文本、RTF/HTML、文件路径、URL 中提取可用文本表示。
- `SelectionResult.bounds`：可选屏幕坐标；macOS Accessibility、Windows UIAutomation 与 Linux AT-SPI2 在 `fetch_bounds = true` 且应用提供文本范围矩形时会返回；降级路径暂保持 `None`。
- `GetTextOptions.allow_clipboard_fallback`：是否允许剪贴板降级。
- `GetTextOptions.max_wait_ms`：等待平台 API 或剪贴板更新的最大毫秒数。
- `GetTextOptions.fetch_bounds`：是否请求坐标。

### `check_permissions()`

```rust
pub fn check_permissions() -> PermissionStatus
```

用于应用启动时检查权限状态：

- macOS：检测 Accessibility 权限。
- Linux/Windows：当前剪贴板降级实现不需要额外权限，返回 `NotRequired`。

### `start_selection_daemon()`

```rust
pub fn start_selection_daemon(
    opts: DaemonOptions,
    on_selection: impl Fn(SelectionResult) + Send + 'static,
) -> Result<DaemonHandle, GetTextError>
```

Push 模式入口：

- macOS 使用 listen-only CGEventTap 监听拖拽选区与双击/三击触发。
- Windows 使用低级 `WH_MOUSE_LL` Hook 监听拖拽选区与双击触发。
- Linux 使用 AT-SPI2 ObjectEvents 触发。
- 不支持的平台或事件源不可用时回退轮询。
- 所有触发路径都复用 `get_selected_text_with_options()`。
- 只在文本或 `bounds` 与上次回调不同的时候触发 `on_selection`。
- `DaemonHandle::stop()` 或 drop 句柄会停止后台线程。

## 错误语义

| 错误 | 含义 | 建议处理 |
| --- | --- | --- |
| `PermissionDenied` | 系统权限不足 | 引导用户开启权限 |
| `NoTextSelected` | 平台明确没有选中文本 | 静默忽略或提示用户先选择文本 |
| `FallbackDisabled` | 调用方禁用了剪贴板降级 | 不再继续模拟复制 |
| `FallbackFailed` | provider 链路失败 | 记录日志并展示通用失败提示 |
| `UnsupportedPlatform` | 当前编译目标未支持 | 禁用相关功能 |

## Push 示例

```rust
use on_selected_text::{start_selection_daemon, DaemonOptions, GetTextOptions};

fn main() {
    let opts = DaemonOptions {
        get_text_options: GetTextOptions::default().with_bounds(),
        ..DaemonOptions::default()
    };

    let _daemon = start_selection_daemon(opts, |selection| {
        println!("selected: {}", selection.text);
    })
    .expect("start selection daemon");
}
```

## 调用示例

```rust
use on_selected_text::{get_selected_text_with_options, GetTextOptions, GetTextError};

fn main() {
    let opts = GetTextOptions::default().with_bounds();

    match get_selected_text_with_options(opts) {
        Ok(selection) => {
            println!("{}", selection.text);
            if let Some(bounds) = selection.bounds {
                println!("bounds: {},{} {}x{}", bounds.x, bounds.y, bounds.width, bounds.height);
            }
        },
        Err(GetTextError::NoTextSelected) => println!("请先选择文本"),
        Err(error) => eprintln!("获取选中文本失败: {error}"),
    }
}
```

## 兼容性约定

1. `get_selected_text()` 的签名与“无选中文本返回空字符串”的行为不得在 `0.x` 小版本中破坏。
2. 新能力优先进入 `get_selected_text_with_options()`，再由旧入口做兼容映射。
3. provider 失败必须尽量映射到 `GetTextError`，避免向上暴露平台库的动态错误字符串。
4. 坐标、Push 模式、原生 A11y 增强应保持 feature/模块边界清晰，避免一次性扩大核心 API。
