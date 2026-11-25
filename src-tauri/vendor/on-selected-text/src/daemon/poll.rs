use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use crate::{get_selected_text_with_options, DaemonOptions, GetTextError, SelectionResult};

const MIN_POLL_INTERVAL_MS: u64 = 10;

/// 轮询 Pull API 并把稳定的新选区推送给回调。
pub(super) fn run_selection_poll_loop(
    opts: DaemonOptions,
    stop_signal: Arc<AtomicBool>,
    on_selection: impl Fn(SelectionResult),
) {
    let interval = poll_interval_from_options(&opts);
    let mut last_selection = None;

    while !stop_signal.load(Ordering::Acquire) {
        thread::sleep(interval);
        if stop_signal.load(Ordering::Acquire) {
            break;
        }

        emit_current_selection(&opts, &mut last_selection, &on_selection);
    }
}

/// 读取一次当前选区并在有变化时回调。
pub(super) fn emit_current_selection(
    opts: &DaemonOptions,
    last_selection: &mut Option<SelectionResult>,
    on_selection: &impl Fn(SelectionResult),
) {
    match get_selected_text_with_options(opts.get_text_options.clone()) {
        Ok(selection) if should_emit_selection(last_selection.as_ref(), &selection) => {
            *last_selection = Some(selection.clone());
            on_selection(selection);
        }
        Ok(_) => {}
        Err(GetTextError::NoTextSelected) => *last_selection = None,
        Err(_) => {}
    }
}

/// 由配置推导轮询间隔，避免 0ms 配置造成后台热循环。
pub(super) fn poll_interval_from_options(opts: &DaemonOptions) -> Duration {
    Duration::from_millis(opts.trigger_delay_ms.max(MIN_POLL_INTERVAL_MS))
}

/// 判断是否需要向调用方推送本次选区。
fn should_emit_selection(
    last_selection: Option<&SelectionResult>,
    next_selection: &SelectionResult,
) -> bool {
    !next_selection.text.is_empty() && last_selection != Some(next_selection)
}

#[cfg(test)]
mod tests {
    use super::{poll_interval_from_options, should_emit_selection, MIN_POLL_INTERVAL_MS};
    use crate::{DaemonOptions, ScreenRect, SelectionResult};

    #[test]
    fn poll_interval_never_allows_hot_loop() {
        let opts = DaemonOptions {
            trigger_delay_ms: 0,
            ..DaemonOptions::default()
        };

        assert_eq!(
            poll_interval_from_options(&opts).as_millis(),
            u128::from(MIN_POLL_INTERVAL_MS)
        );
    }

    #[test]
    fn poll_interval_uses_configured_delay() {
        let opts = DaemonOptions {
            trigger_delay_ms: 42,
            ..DaemonOptions::default()
        };

        assert_eq!(poll_interval_from_options(&opts).as_millis(), 42);
    }

    #[test]
    fn should_emit_selection_ignores_empty_text() {
        assert!(!should_emit_selection(None, &SelectionResult::new("")));
    }

    #[test]
    fn should_emit_selection_deduplicates_same_result() {
        let selection = SelectionResult::new("hello");

        assert!(should_emit_selection(None, &selection));
        assert!(!should_emit_selection(Some(&selection), &selection));
    }

    #[test]
    fn should_emit_selection_treats_bounds_changes_as_new_selection() {
        let previous = SelectionResult::new("hello");
        let next = SelectionResult::new("hello").with_bounds(ScreenRect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        });

        assert!(should_emit_selection(Some(&previous), &next));
    }
}
