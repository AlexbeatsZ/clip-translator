/// 选中文本在屏幕坐标系中的边界。
///
/// 坐标用于 PopClip 类浮窗定位。不同平台或应用可能无法提供坐标，
/// 因此它通过 [`SelectionResult::bounds`] 以 `Option` 暴露。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    /// 左上角横坐标，单位为物理屏幕点/像素，由平台决定。
    pub x: f64,
    /// 左上角纵坐标，单位为物理屏幕点/像素，由平台决定。
    pub y: f64,
    /// 选区包围盒宽度。
    pub width: f64,
    /// 选区包围盒高度。
    pub height: f64,
}

/// 选中文本提取结果。
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionResult {
    /// 提取到的选中文本。
    pub text: String,
    /// 可选屏幕坐标；`None` 表示当前平台/应用暂不支持坐标提取。
    pub bounds: Option<ScreenRect>,
}

impl SelectionResult {
    /// 创建仅包含文本的结果。
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bounds: None,
        }
    }

    /// 返回附带屏幕坐标的新结果。
    pub fn with_bounds(mut self, bounds: ScreenRect) -> Self {
        self.bounds = Some(bounds);
        self
    }
}
