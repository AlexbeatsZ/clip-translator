use std::{
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard};
use parking_lot::{Mutex, MutexGuard};

use crate::GetTextError;

static CLIPBOARD_MUTEX: Mutex<()> = Mutex::new(());
static INPUT_MUTEX: Mutex<()> = Mutex::new(());

#[cfg(target_os = "windows")]
type ClipboardSnapshot = super::windows_clipboard::ClipboardSnapshot;

/// 剪贴板快照，仅覆盖当前 arboard 能稳定备份/还原的文本与图片格式。
#[cfg(not(target_os = "windows"))]
enum ClipboardSnapshot {
    Text(String),
    Image(arboard::ImageData<'static>),
    Empty,
}

/// 剪贴板事务保护器。
///
/// 该结构通过 Drop 自动还原剪贴板，并用进程级互斥锁避免并发调用互相踩踏。
pub(crate) struct ClipboardTransaction {
    _guard: MutexGuard<'static, ()>,
    snapshot: ClipboardSnapshot,
    placeholder: String,
}

impl ClipboardTransaction {
    /// 开始一次剪贴板事务：加锁、备份、写入唯一占位符。
    pub(crate) fn begin() -> Result<Self, GetTextError> {
        let guard = CLIPBOARD_MUTEX
            .try_lock_for(Duration::from_secs(2))
            .ok_or_else(|| GetTextError::fallback_failed("clipboard mutex timeout"))?;
        let mut clipboard = Clipboard::new().map_err(to_clipboard_error)?;
        let snapshot = backup_clipboard(&mut clipboard)?;
        let placeholder = build_placeholder();

        clipboard
            .set_text(placeholder.clone())
            .map_err(to_clipboard_error)?;
        write_platform_clipboard_markers()?;
        thread::sleep(Duration::from_millis(16));

        Ok(Self {
            _guard: guard,
            snapshot,
            placeholder,
        })
    }

    /// 模拟复制并轮询剪贴板变化，最后返回提取到的文本。
    pub(crate) fn simulate_copy_and_read(
        &self,
        enigo: &mut Enigo,
        max_wait_ms: u64,
        cancel_select: bool,
    ) -> Result<String, GetTextError> {
        copy(enigo)?;

        if cancel_select {
            right_arrow_click(enigo, 1)?;
        }

        self.wait_for_text_change(max_wait_ms)
    }

    /// 轮询直到剪贴板文本与占位符不同，避免固定 sleep 带来的慢或不稳定。
    fn wait_for_text_change(&self, max_wait_ms: u64) -> Result<String, GetTextError> {
        let timeout = Duration::from_millis(max_wait_ms.max(1));
        let deadline = Instant::now() + timeout;
        let mut clipboard = Clipboard::new().map_err(to_clipboard_error)?;

        loop {
            if let Ok(text) = clipboard.get_text() {
                if text != self.placeholder {
                    return if text.is_empty() {
                        Err(GetTextError::NoTextSelected)
                    } else {
                        write_platform_clipboard_markers()?;
                        Ok(text)
                    };
                }
            }

            if Instant::now() >= deadline {
                return Err(GetTextError::NoTextSelected);
            }

            thread::sleep(Duration::from_millis(8));
        }
    }
}

impl Drop for ClipboardTransaction {
    fn drop(&mut self) {
        if let Err(error) = restore_clipboard(&self.snapshot) {
            eprintln!("[get-selected-text] WARN: clipboard restore failed: {error}");
        }
    }
}

/// 创建 Enigo 输入控制器。
pub(crate) fn create_enigo() -> Result<Enigo, GetTextError> {
    Enigo::new(&Default::default())
        .map_err(|error| GetTextError::fallback_failed(format!("create input controller: {error}")))
}

/// 写入平台特定的剪贴板防污染标记。
fn write_platform_clipboard_markers() -> Result<(), GetTextError> {
    #[cfg(target_os = "windows")]
    {
        super::windows_clipboard::write_cloud_history_blocking_markers()
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

/// 备份当前剪贴板中可恢复的主要格式。
#[cfg(target_os = "windows")]
fn backup_clipboard(_clipboard: &mut Clipboard) -> Result<ClipboardSnapshot, GetTextError> {
    super::windows_clipboard::backup_clipboard()
}

/// 备份当前剪贴板中 arboard 可表达的主要格式。
#[cfg(not(target_os = "windows"))]
fn backup_clipboard(clipboard: &mut Clipboard) -> Result<ClipboardSnapshot, GetTextError> {
    let snapshot = if let Ok(text) = clipboard.get_text() {
        ClipboardSnapshot::Text(text)
    } else if let Ok(image) = clipboard.get_image() {
        ClipboardSnapshot::Image(image)
    } else {
        ClipboardSnapshot::Empty
    };

    Ok(snapshot)
}

/// 根据快照还原剪贴板内容。
#[cfg(target_os = "windows")]
fn restore_clipboard(snapshot: &ClipboardSnapshot) -> Result<(), GetTextError> {
    super::windows_clipboard::restore_clipboard(snapshot)
}

/// 根据快照还原剪贴板内容。
#[cfg(not(target_os = "windows"))]
fn restore_clipboard(snapshot: &ClipboardSnapshot) -> Result<(), GetTextError> {
    let mut clipboard = Clipboard::new().map_err(to_clipboard_error)?;

    match snapshot {
        ClipboardSnapshot::Text(text) => {
            clipboard.set_text(text.clone()).map_err(to_clipboard_error)
        }
        ClipboardSnapshot::Image(image) => clipboard
            .set_image(image.clone())
            .map_err(to_clipboard_error),
        ClipboardSnapshot::Empty => clipboard.clear().map_err(to_clipboard_error),
    }
}

/// 构造进程内唯一占位符，用于判断复制动作是否真正更新了剪贴板。
fn build_placeholder() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!(
        "__GET_SELECTED_TEXT_SENTINEL_{}_{}__",
        std::process::id(),
        nanos
    )
}

/// 释放可能卡住的修饰键，减少模拟复制前的输入状态污染。
fn release_control_keys(enigo: &mut Enigo) -> Result<(), GetTextError> {
    for key in [Key::Control, Key::Alt, Key::Shift, Key::Space, Key::Tab] {
        enigo.key(key, Direction::Release).map_err(to_input_error)?;
    }
    Ok(())
}

/// 模拟 Ctrl+C 复制当前选区。
fn copy(enigo: &mut Enigo) -> Result<(), GetTextError> {
    let _guard = INPUT_MUTEX.lock();

    release_control_keys(enigo)?;
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(to_input_error)?;
    #[cfg(target_os = "windows")]
    enigo
        .key(Key::C, Direction::Click)
        .map_err(to_input_error)?;
    #[cfg(target_os = "linux")]
    enigo
        .key(Key::Unicode('c'), Direction::Click)
        .map_err(to_input_error)?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(to_input_error)?;

    Ok(())
}

/// 通过右方向键取消选区，保留旧实现中的可选行为。
fn right_arrow_click(enigo: &mut Enigo, count: usize) -> Result<(), GetTextError> {
    let _guard = INPUT_MUTEX.lock();

    for _ in 0..count {
        enigo
            .key(Key::RightArrow, Direction::Click)
            .map_err(to_input_error)?;
    }

    Ok(())
}

/// 统一包装剪贴板错误。
fn to_clipboard_error(error: arboard::Error) -> GetTextError {
    GetTextError::fallback_failed(format!("clipboard operation failed: {error}"))
}

/// 统一包装输入模拟错误。
fn to_input_error(error: enigo::InputError) -> GetTextError {
    GetTextError::fallback_failed(format!("input simulation failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::build_placeholder;

    #[test]
    fn placeholder_is_unique_enough_for_runtime_checks() {
        let first = build_placeholder();
        let second = build_placeholder();

        assert_ne!(first, second);
        assert!(first.contains("GET_SELECTED_TEXT_SENTINEL"));
    }
}
