/// Pull 模式下获取选中文本的配置项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTextOptions {
    /// 是否允许在原生无障碍 API 不可用时降级到剪贴板模拟复制。
    pub allow_clipboard_fallback: bool,
    /// 等待剪贴板或平台 API 更新的最大时长，单位毫秒。
    pub max_wait_ms: u64,
    /// 是否尝试同时获取选区屏幕坐标。
    pub fetch_bounds: bool,
}

impl Default for GetTextOptions {
    fn default() -> Self {
        Self {
            allow_clipboard_fallback: true,
            max_wait_ms: 500,
            fetch_bounds: false,
        }
    }
}

impl GetTextOptions {
    /// 创建禁用剪贴板降级的选项，适合对剪贴板零副作用有强约束的调用方。
    pub fn without_clipboard_fallback(mut self) -> Self {
        self.allow_clipboard_fallback = false;
        self
    }

    /// 创建启用坐标提取的选项。
    pub fn with_bounds(mut self) -> Self {
        self.fetch_bounds = true;
        self
    }
}

/// Push 模式守护进程配置。
///
/// 当前版本先公开稳定配置结构，平台事件监听会在后续 feature 中逐步接入。
#[derive(Debug, Clone, PartialEq)]
pub struct DaemonOptions {
    /// 鼠标位移防抖阈值，低于该值不认为是拖拽选区。
    pub drag_threshold_px: f64,
    /// 鼠标抬起后等待无障碍树更新的延迟，单位毫秒。
    pub trigger_delay_ms: u64,
    /// 触发后实际读取选中文本时使用的 Pull 模式配置。
    pub get_text_options: GetTextOptions,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            drag_threshold_px: 3.0,
            trigger_delay_ms: 30,
            get_text_options: GetTextOptions::default(),
        }
    }
}

/// 当前平台读取选中文本所需权限的状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionStatus {
    /// 当前进程已具备所需权限。
    Granted,
    /// 当前进程缺失权限，字段携带可展示给用户的指引。
    Denied {
        /// 平台名称。
        platform: &'static str,
        /// 权限名称。
        permission: &'static str,
        /// 用户指引。
        guidance: &'static str,
    },
    /// 当前平台/当前实现不需要额外权限。
    NotRequired,
    /// 当前平台尚未实现权限检测。
    Unsupported,
}
