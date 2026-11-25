use crate::{GetTextError, GetTextOptions, SelectionResult};

/// 平台文本提取策略的最小抽象。
///
/// 每个平台可以按“原生 API → 剪贴板降级”的顺序组合多个 provider。
pub(crate) trait TextProvider {
    /// provider 的稳定名称，用于错误聚合与日志定位。
    fn name(&self) -> &'static str;

    /// 尝试用当前 provider 提取选中文本。
    fn try_get_text(&self, opts: &GetTextOptions) -> Result<SelectionResult, GetTextError>;
}

/// 按顺序执行 provider 链，返回首个成功结果。
pub(crate) fn run_provider_chain<'a, I>(
    providers: I,
    opts: &GetTextOptions,
) -> Result<SelectionResult, GetTextError>
where
    I: IntoIterator<Item = &'a dyn TextProvider>,
{
    let mut failures = Vec::new();

    for provider in providers {
        match provider.try_get_text(opts) {
            Ok(result) if result.text.is_empty() => return Err(GetTextError::NoTextSelected),
            Ok(result) => return Ok(result),
            Err(GetTextError::NoTextSelected) => return Err(GetTextError::NoTextSelected),
            Err(GetTextError::PermissionDenied {
                platform,
                permission,
                guidance,
            }) => {
                return Err(GetTextError::PermissionDenied {
                    platform,
                    permission,
                    guidance,
                });
            }
            Err(error) => failures.push(format!("{}: {error}", provider.name())),
        }
    }

    if failures.is_empty() {
        Err(GetTextError::UnsupportedPlatform(std::env::consts::OS))
    } else {
        Err(GetTextError::fallback_failed(failures.join("; ")))
    }
}
