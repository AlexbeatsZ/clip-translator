use crate::{GetTextError, GetTextOptions, PermissionStatus, SelectionResult};

#[cfg(not(target_os = "macos"))]
pub(crate) mod clipboard_guard;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_clipboard;
#[cfg(target_os = "macos")]
mod macos_pasteboard_text;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
mod windows_clipboard;

/// 按当前编译目标分发到平台实现。
pub(crate) fn get_selected_text_with_options(
    opts: &GetTextOptions,
) -> Result<SelectionResult, GetTextError> {
    #[cfg(target_os = "linux")]
    {
        return linux::get_selected_text_with_options(opts);
    }

    #[cfg(target_os = "macos")]
    {
        return macos::get_selected_text_with_options(opts);
    }

    #[cfg(target_os = "windows")]
    {
        return windows::get_selected_text_with_options(opts);
    }

    #[allow(unreachable_code)]
    Err(GetTextError::UnsupportedPlatform(std::env::consts::OS))
}

/// 按当前编译目标检查权限状态。
pub(crate) fn check_permissions() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        return macos::check_permissions();
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        return PermissionStatus::NotRequired;
    }

    #[allow(unreachable_code)]
    PermissionStatus::Unsupported
}
