use crate::{
    provider::{run_provider_chain, TextProvider},
    GetTextError, GetTextOptions, ScreenRect, SelectionResult,
};

use super::clipboard_guard::{create_enigo, ClipboardTransaction};

/// Windows UIAutomation 文本直读 provider。
///
/// UIAutomation 通过当前焦点元素的 TextPattern 读取选区，不修改剪贴板；
/// 若当前应用不支持 TextPattern，则交给剪贴板 fallback。
struct WindowsAccessibilityProvider;

impl TextProvider for WindowsAccessibilityProvider {
    fn name(&self) -> &'static str {
        "windows.uiautomation"
    }

    fn try_get_text(&self, opts: &GetTextOptions) -> Result<SelectionResult, GetTextError> {
        read_windows_uia_selection(opts.fetch_bounds).map_err(|error| match error {
            GetTextError::NoTextSelected => GetTextError::fallback_failed(
                "Windows UIAutomation focused text selection is empty",
            ),
            other => other,
        })
    }
}

/// Windows 当前实现的剪贴板降级 provider。
struct WindowsClipboardProvider;

impl TextProvider for WindowsClipboardProvider {
    fn name(&self) -> &'static str {
        "windows.clipboard"
    }

    fn try_get_text(&self, opts: &GetTextOptions) -> Result<SelectionResult, GetTextError> {
        if !opts.allow_clipboard_fallback {
            return Err(GetTextError::FallbackDisabled);
        }

        let mut enigo = create_enigo()?;
        let transaction = ClipboardTransaction::begin()?;
        let text = transaction.simulate_copy_and_read(&mut enigo, opts.max_wait_ms, false)?;

        Ok(SelectionResult::new(text))
    }
}

/// Windows 平台入口。
pub(crate) fn get_selected_text_with_options(
    opts: &GetTextOptions,
) -> Result<SelectionResult, GetTextError> {
    let accessibility = WindowsAccessibilityProvider;
    let clipboard = WindowsClipboardProvider;
    run_provider_chain(
        [
            &accessibility as &dyn TextProvider,
            &clipboard as &dyn TextProvider,
        ],
        opts,
    )
}

/// 通过 Windows UIAutomation 读取当前焦点元素的第一段选区。
fn read_windows_uia_selection(fetch_bounds: bool) -> Result<SelectionResult, GetTextError> {
    use windows::{
        core::Interface,
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_APARTMENTTHREADED,
            },
            UI::Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
                IUIAutomationTextPattern2, UIA_TextPattern2Id, UIA_TextPatternId,
            },
        },
    };

    /// 优先使用 TextPattern2；不支持时回退到基础 TextPattern。
    unsafe fn get_text_pattern(
        element: &IUIAutomationElement,
    ) -> Result<IUIAutomationTextPattern, GetTextError> {
        if let Some(text_pattern) = try_get_text_pattern2(element) {
            return Ok(text_pattern);
        }

        element
            .GetCurrentPatternAs(UIA_TextPatternId)
            .map_err(|error| GetTextError::fallback_failed(format!("get TextPattern: {error}")))
    }

    /// 尝试把 TextPattern2 作为基础 TextPattern 使用；失败不阻断后续 TextPattern。
    unsafe fn try_get_text_pattern2(
        element: &IUIAutomationElement,
    ) -> Option<IUIAutomationTextPattern> {
        element
            .GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
            .ok()?
            .cast()
            .ok()
    }

    struct ComApartment {
        should_uninitialize: bool,
    }

    impl ComApartment {
        /// 初始化当前线程的 COM apartment；若调用方已使用不同模式初始化，则复用现状。
        unsafe fn init() -> Result<Self, GetTextError> {
            let result = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if result == RPC_E_CHANGED_MODE {
                return Ok(Self {
                    should_uninitialize: false,
                });
            }

            result.ok().map_err(|error| {
                GetTextError::fallback_failed(format!("initialize COM: {error}"))
            })?;
            Ok(Self {
                should_uninitialize: true,
            })
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.should_uninitialize {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    unsafe {
        let _apartment = ComApartment::init()?;
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|error| {
                GetTextError::fallback_failed(format!("create UIAutomation instance: {error}"))
            })?;
        let element = automation.GetFocusedElement().map_err(|error| {
            GetTextError::fallback_failed(format!("get focused element: {error}"))
        })?;
        let text_pattern = get_text_pattern(&element)?;
        let ranges = text_pattern.GetSelection().map_err(|error| {
            GetTextError::fallback_failed(format!("get text selection: {error}"))
        })?;
        let range_count = ranges.Length().map_err(|error| {
            GetTextError::fallback_failed(format!("get selection range count: {error}"))
        })?;
        if range_count <= 0 {
            return Err(GetTextError::NoTextSelected);
        }

        let first_range = ranges.GetElement(0).map_err(|error| {
            GetTextError::fallback_failed(format!("get first text range: {error}"))
        })?;
        let selected_text = first_range.GetText(-1).map_err(|error| {
            GetTextError::fallback_failed(format!("read selected text: {error}"))
        })?;
        let text =
            normalize_windows_uia_text(String::try_from(selected_text).map_err(|error| {
                GetTextError::fallback_failed(format!("decode UIAutomation BSTR: {error}"))
            })?)?;

        let mut selection = SelectionResult::new(text);
        if fetch_bounds {
            if let Some(bounds) = read_windows_uia_bounds(&first_range) {
                selection = selection.with_bounds(bounds);
            }
        }

        Ok(selection)
    }
}

/// 读取 UIAutomation TextRange 返回的屏幕矩形；坐标失败不影响文本结果。
unsafe fn read_windows_uia_bounds(
    range: &windows::Win32::UI::Accessibility::IUIAutomationTextRange,
) -> Option<ScreenRect> {
    let rectangles = range.GetBoundingRectangles().ok()?;
    let values = safe_array_to_f64_vec(rectangles).ok()?;
    uia_rect_values_to_bounds(&values)
}

/// 将 UIAutomation 的扁平 `[left, top, width, height, ...]` 坐标合并为包围盒。
fn uia_rect_values_to_bounds(values: &[f64]) -> Option<ScreenRect> {
    let rect_chunks = values.chunks_exact(4);
    if values.is_empty() || !rect_chunks.remainder().is_empty() {
        return None;
    }

    let mut union: Option<(f64, f64, f64, f64)> = None;
    for rect in rect_chunks {
        let (x, y, width, height) = (rect[0], rect[1], rect[2], rect[3]);
        let right = x + width;
        let bottom = y + height;
        if ![x, y, width, height, right, bottom]
            .iter()
            .all(|value| value.is_finite())
            || width <= 0.0
            || height <= 0.0
        {
            continue;
        }

        union = Some(match union {
            Some((left, top, current_right, current_bottom)) => (
                left.min(x),
                top.min(y),
                current_right.max(right),
                current_bottom.max(bottom),
            ),
            None => (x, y, right, bottom),
        });
    }

    union.map(|(x, y, right, bottom)| ScreenRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

/// 复制并释放 UIAutomation 返回的 SAFEARRAY<double>。
unsafe fn safe_array_to_f64_vec(
    array: *mut windows::Win32::System::Com::SAFEARRAY,
) -> Result<Vec<f64>, GetTextError> {
    use std::ffi::c_void;
    use windows::Win32::System::Ole::{
        SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetElemsize,
        SafeArrayGetLBound, SafeArrayGetUBound,
    };

    struct OwnedSafeArray(*mut windows::Win32::System::Com::SAFEARRAY);

    impl OwnedSafeArray {
        fn new(array: *mut windows::Win32::System::Com::SAFEARRAY) -> Result<Self, GetTextError> {
            if array.is_null() {
                Err(GetTextError::fallback_failed(
                    "UIAutomation returned null bounding rectangles array",
                ))
            } else {
                Ok(Self(array))
            }
        }

        fn as_ptr(&self) -> *mut windows::Win32::System::Com::SAFEARRAY {
            self.0
        }
    }

    impl Drop for OwnedSafeArray {
        fn drop(&mut self) {
            unsafe {
                let _ = SafeArrayDestroy(self.0);
            }
        }
    }

    let array = OwnedSafeArray::new(array)?;
    let dimensions = SafeArrayGetDim(array.as_ptr());
    if dimensions != 1 {
        return Err(GetTextError::fallback_failed(format!(
            "bounding rectangles array has {dimensions} dimensions"
        )));
    }

    let element_size = SafeArrayGetElemsize(array.as_ptr()) as usize;
    if element_size != std::mem::size_of::<f64>() {
        return Err(GetTextError::fallback_failed(format!(
            "bounding rectangles element size is {element_size} bytes"
        )));
    }

    let lower_bound = SafeArrayGetLBound(array.as_ptr(), 1).map_err(|error| {
        GetTextError::fallback_failed(format!("read bounding rectangles lower bound: {error}"))
    })?;
    let upper_bound = SafeArrayGetUBound(array.as_ptr(), 1).map_err(|error| {
        GetTextError::fallback_failed(format!("read bounding rectangles upper bound: {error}"))
    })?;
    if upper_bound < lower_bound {
        return Ok(Vec::new());
    }

    let len = (i64::from(upper_bound) - i64::from(lower_bound) + 1) as usize;
    let mut values = Vec::with_capacity(len);
    for index in lower_bound..=upper_bound {
        let mut value = 0.0_f64;
        SafeArrayGetElement(
            array.as_ptr(),
            &index,
            (&mut value as *mut f64).cast::<c_void>(),
        )
        .map_err(|error| {
            GetTextError::fallback_failed(format!("read bounding rectangle element: {error}"))
        })?;
        values.push(value);
    }

    Ok(values)
}

/// 规范化 UIAutomation 返回的选中文本。
fn normalize_windows_uia_text(text: String) -> Result<String, GetTextError> {
    let text = text.trim_matches('\0').to_string();
    if text.is_empty() {
        Err(GetTextError::NoTextSelected)
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_windows_uia_text, uia_rect_values_to_bounds};
    use crate::{GetTextError, ScreenRect};

    #[test]
    fn normalize_windows_uia_text_strips_nul_padding_only() {
        let text = normalize_windows_uia_text("  hello  \0".to_string()).unwrap();

        assert_eq!(text, "  hello  ");
    }

    #[test]
    fn normalize_windows_uia_text_reports_empty_selection() {
        let error = normalize_windows_uia_text("\0\0".to_string()).unwrap_err();

        assert_eq!(error, GetTextError::NoTextSelected);
    }

    #[test]
    fn uia_rect_values_to_bounds_unions_wrapped_lines() {
        let bounds = uia_rect_values_to_bounds(&[10.0, 20.0, 30.0, 5.0, 15.0, 30.0, 20.0, 10.0]);

        assert_eq!(
            bounds,
            Some(ScreenRect {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 20.0,
            })
        );
    }

    #[test]
    fn uia_rect_values_to_bounds_ignores_invalid_rectangles() {
        let bounds = uia_rect_values_to_bounds(&[0.0, 0.0, -1.0, 5.0, 4.0, 5.0, 6.0, 7.0]);

        assert_eq!(
            bounds,
            Some(ScreenRect {
                x: 4.0,
                y: 5.0,
                width: 6.0,
                height: 7.0,
            })
        );
    }

    #[test]
    fn uia_rect_values_to_bounds_rejects_incomplete_input() {
        assert_eq!(uia_rect_values_to_bounds(&[1.0, 2.0, 3.0]), None);
    }
}
