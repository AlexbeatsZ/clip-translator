//! Cross-platform selected text retrieval.
//!
//! The crate keeps the original [`get_selected_text`] convenience API while
//! adding a typed result API via [`get_selected_text_with_options`]. Platform
//! implementations are selected with conditional compilation.

mod daemon;
mod error;
mod options;
mod platform;
#[cfg(not(target_os = "macos"))]
mod provider;
mod result;

pub use daemon::{start_selection_daemon, DaemonHandle};
pub use error::GetTextError;
pub use options::{DaemonOptions, GetTextOptions, PermissionStatus};
pub use result::{ScreenRect, SelectionResult};

/// 获取当前选中文本的兼容入口。
///
/// 为了兼容旧版本行为，“没有选中文本”会返回 `Ok(String::new())`；
/// 新代码若需要精确区分失败原因，请使用 [`get_selected_text_with_options`]。
///
/// # Example
///
/// ```no_run
/// use on_selected_text::get_selected_text;
///
/// let text = get_selected_text().unwrap();
/// println!("selected text: {}", text);
/// ```
pub fn get_selected_text() -> Result<String, Box<dyn std::error::Error>> {
    match get_selected_text_with_options(GetTextOptions::default()) {
        Ok(selection) => Ok(selection.text),
        Err(GetTextError::NoTextSelected) => Ok(String::new()),
        Err(error) => Err(Box::new(error)),
    }
}

/// 使用配置项获取当前选中文本及可选屏幕坐标。
///
/// 该 API 返回 [`GetTextError`]，可以精确区分权限缺失、无选中文本、
/// 降级失败与平台不支持。
pub fn get_selected_text_with_options(
    opts: GetTextOptions,
) -> Result<SelectionResult, GetTextError> {
    platform::get_selected_text_with_options(&opts)
}

/// 检查当前平台读取选中文本所需的权限状态。
pub fn check_permissions() -> PermissionStatus {
    platform::check_permissions()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_keep_clipboard_fallback_enabled() {
        let opts = GetTextOptions::default();

        assert!(opts.allow_clipboard_fallback);
        assert_eq!(opts.max_wait_ms, 500);
        assert!(!opts.fetch_bounds);
    }

    #[test]
    fn option_builders_are_chainable() {
        let opts = GetTextOptions::default()
            .without_clipboard_fallback()
            .with_bounds();

        assert!(!opts.allow_clipboard_fallback);
        assert!(opts.fetch_bounds);
    }

    #[test]
    fn selection_result_can_attach_bounds() {
        let bounds = ScreenRect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        };

        let result = SelectionResult::new("hello").with_bounds(bounds);

        assert_eq!(result.text, "hello");
        assert_eq!(result.bounds, Some(bounds));
    }
}
