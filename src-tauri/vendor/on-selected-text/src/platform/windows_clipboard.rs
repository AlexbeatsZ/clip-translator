use std::{ptr, slice};

use windows::{
    core::w,
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
                OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
            },
            Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
        },
    },
};

use crate::GetTextError;

const CF_BITMAP: u32 = 2;
const CF_ENHMETAFILE: u32 = 14;
const CF_OWNERDISPLAY: u32 = 128;
const CF_PRIVATEFIRST: u32 = 0x0200;
const CF_PRIVATELAST: u32 = 0x02ff;
const CF_GDIOBJFIRST: u32 = 0x0300;
const CF_GDIOBJLAST: u32 = 0x03ff;

#[cfg(test)]
const CLIPBOARD_HISTORY_FORMAT: &str = "CanIncludeInClipboardHistory";
#[cfg(test)]
const MONITOR_PROCESSING_FORMAT: &str = "ExcludeClipboardContentFromMonitorProcessing";

/// Windows 原始剪贴板快照。
///
/// 只保存可安全复制为 HGLOBAL 字节块的格式；GDI/owner/private 格式会被跳过，
/// 避免把宿主进程指针或图形对象句柄错误还原到剪贴板。
pub(crate) struct ClipboardSnapshot {
    entries: Vec<ClipboardEntry>,
}

struct ClipboardEntry {
    format: u32,
    bytes: Vec<u8>,
}

/// 备份当前 Windows 剪贴板中可安全深拷贝的格式。
pub(crate) fn backup_clipboard() -> Result<ClipboardSnapshot, GetTextError> {
    let _guard = ClipboardOpenGuard::open()?;
    let mut entries = Vec::new();
    let mut format = 0;

    loop {
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }

        if should_skip_format(format) {
            continue;
        }

        if let Some(entry) = unsafe { copy_clipboard_format(format) }? {
            entries.push(entry);
        }
    }

    Ok(ClipboardSnapshot { entries })
}

/// 用快照还原 Windows 剪贴板。
pub(crate) fn restore_clipboard(snapshot: &ClipboardSnapshot) -> Result<(), GetTextError> {
    let _guard = ClipboardOpenGuard::open()?;
    unsafe {
        EmptyClipboard().map_err(|error| {
            GetTextError::fallback_failed(format!("empty Windows clipboard: {error}"))
        })?;
    }

    for entry in &snapshot.entries {
        unsafe {
            let memory = allocate_global_bytes(&entry.bytes)?;
            if let Err(error) = SetClipboardData(entry.format, HANDLE(memory.0)) {
                let _ = GlobalFree(memory);
                return Err(GetTextError::fallback_failed(format!(
                    "restore Windows clipboard format {}: {error}",
                    entry.format
                )));
            }
        }
    }

    Ok(())
}

/// 写入 Windows 剪贴板历史和云同步阻断 marker。
pub(crate) fn write_cloud_history_blocking_markers() -> Result<(), GetTextError> {
    let _guard = ClipboardOpenGuard::open()?;
    unsafe {
        set_u32_marker(RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")))?;
        set_u32_marker(RegisterClipboardFormatW(w!(
            "ExcludeClipboardContentFromMonitorProcessing"
        )))?;
    }
    Ok(())
}

struct ClipboardOpenGuard;

impl ClipboardOpenGuard {
    fn open() -> Result<Self, GetTextError> {
        unsafe {
            OpenClipboard(HWND::default()).map_err(|error| {
                GetTextError::fallback_failed(format!("open Windows clipboard: {error}"))
            })?;
        }
        Ok(Self)
    }
}

impl Drop for ClipboardOpenGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

unsafe fn copy_clipboard_format(format: u32) -> Result<Option<ClipboardEntry>, GetTextError> {
    let handle = match GetClipboardData(format) {
        Ok(handle) => handle,
        Err(_) => return Ok(None),
    };
    let memory = HGLOBAL(handle.0);
    let size = GlobalSize(memory);
    if size == 0 {
        return Ok(None);
    }

    let ptr = GlobalLock(memory) as *const u8;
    if ptr.is_null() {
        return Ok(None);
    }

    let bytes = slice::from_raw_parts(ptr, size).to_vec();
    let _ = GlobalUnlock(memory);

    Ok(Some(ClipboardEntry { format, bytes }))
}

unsafe fn set_u32_marker(format: u32) -> Result<(), GetTextError> {
    if format == 0 {
        return Err(GetTextError::fallback_failed(
            "register Windows clipboard marker format returned 0",
        ));
    }

    let bytes = 0u32.to_ne_bytes();
    let memory = allocate_global_bytes(&bytes)?;
    if let Err(error) = SetClipboardData(format, HANDLE(memory.0)) {
        let _ = GlobalFree(memory);
        return Err(GetTextError::fallback_failed(format!(
            "set Windows clipboard marker: {error}"
        )));
    }

    Ok(())
}

unsafe fn allocate_global_bytes(bytes: &[u8]) -> Result<HGLOBAL, GetTextError> {
    let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|error| {
        GetTextError::fallback_failed(format!("allocate Windows clipboard memory: {error}"))
    })?;
    let ptr = GlobalLock(memory) as *mut u8;
    if ptr.is_null() {
        let _ = GlobalFree(memory);
        return Err(GetTextError::fallback_failed(
            "lock Windows clipboard memory failed",
        ));
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    let _ = GlobalUnlock(memory);

    Ok(memory)
}

/// 判断是否属于不能安全按 HGLOBAL 字节块保存/还原的格式。
fn should_skip_format(format: u32) -> bool {
    matches!(format, CF_BITMAP | CF_ENHMETAFILE | CF_OWNERDISPLAY)
        || (CF_PRIVATEFIRST..=CF_PRIVATELAST).contains(&format)
        || (CF_GDIOBJFIRST..=CF_GDIOBJLAST).contains(&format)
}

#[cfg(test)]
mod tests {
    use super::{
        should_skip_format, CF_BITMAP, CF_ENHMETAFILE, CF_GDIOBJFIRST, CF_OWNERDISPLAY,
        CF_PRIVATEFIRST, CLIPBOARD_HISTORY_FORMAT, MONITOR_PROCESSING_FORMAT,
    };

    #[test]
    fn marker_format_names_match_windows_contract() {
        assert_eq!(CLIPBOARD_HISTORY_FORMAT, "CanIncludeInClipboardHistory");
        assert_eq!(
            MONITOR_PROCESSING_FORMAT,
            "ExcludeClipboardContentFromMonitorProcessing"
        );
    }

    #[test]
    fn skip_dangerous_non_global_or_private_formats() {
        assert!(should_skip_format(CF_BITMAP));
        assert!(should_skip_format(CF_ENHMETAFILE));
        assert!(should_skip_format(CF_OWNERDISPLAY));
        assert!(should_skip_format(CF_PRIVATEFIRST));
        assert!(should_skip_format(CF_GDIOBJFIRST));
        assert!(!should_skip_format(13));
    }
}
