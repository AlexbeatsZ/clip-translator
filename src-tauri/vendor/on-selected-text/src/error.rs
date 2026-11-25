use std::fmt;

/// 获取选中文本时的精确错误类型。
///
/// 设计目标是区分“确实没有选中文本”和“平台能力/降级链路失败”，
/// 让调用方可以给用户更准确的提示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetTextError {
    /// 当前进程缺少读取选中文本所需的系统权限。
    PermissionDenied {
        /// 发生权限问题的平台名称。
        platform: &'static str,
        /// 缺失的权限名称。
        permission: &'static str,
        /// 给最终用户展示的开启权限指引。
        guidance: &'static str,
    },
    /// 平台明确反馈当前没有选中文本。
    NoTextSelected,
    /// 调用方禁止使用剪贴板降级策略。
    FallbackDisabled,
    /// 所有可用策略均失败，字符串中保留关键失败原因。
    FallbackFailed(String),
    /// 当前编译目标尚未支持。
    UnsupportedPlatform(&'static str),
}

impl fmt::Display for GetTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PermissionDenied {
                platform,
                permission,
                guidance,
            } => write!(
                formatter,
                "{platform} requires {permission} permission to read selected text: {guidance}"
            ),
            Self::NoTextSelected => formatter.write_str("no text is currently selected"),
            Self::FallbackDisabled => {
                formatter.write_str("clipboard fallback is disabled by options")
            }
            Self::FallbackFailed(reason) => {
                write!(formatter, "selected text fallback failed: {reason}")
            }
            Self::UnsupportedPlatform(platform) => {
                write!(formatter, "selected text is unsupported on {platform}")
            }
        }
    }
}

impl std::error::Error for GetTextError {}

impl GetTextError {
    /// 使用统一前缀包装平台内部错误，避免在平台模块重复拼接字符串。
    pub(crate) fn fallback_failed(reason: impl Into<String>) -> Self {
        Self::FallbackFailed(reason.into())
    }
}
