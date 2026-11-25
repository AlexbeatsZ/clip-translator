use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
        Arc,
    },
    thread,
};

use core_foundation::runloop::{kCFRunLoopCommonModes, kCFRunLoopDefaultMode, CFRunLoop};
use core_graphics::{
    event::{
        CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventType, EventField,
    },
    geometry::CGPoint,
};
use parking_lot::Mutex;

use crate::{DaemonOptions, GetTextError, SelectionResult};

use super::poll::{emit_current_selection, poll_interval_from_options};

/// 使用 macOS CGEventTap 监听鼠标选区动作；不可用时返回错误交给轮询 fallback。
pub(super) fn run_macos_event_tap_loop(
    opts: DaemonOptions,
    stop_signal: Arc<AtomicBool>,
    on_selection: &impl Fn(SelectionResult),
) -> Result<(), GetTextError> {
    let interval = poll_interval_from_options(&opts);
    let (trigger_tx, trigger_rx) = std::sync::mpsc::channel();
    let mouse_state = Arc::new(Mutex::new(MacosMouseTriggerState::new(
        opts.drag_threshold_px,
    )));
    let tap_mouse_state = Arc::clone(&mouse_state);
    let tap_trigger_tx = trigger_tx.clone();

    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::LeftMouseDown, CGEventType::LeftMouseUp],
        move |_proxy, event_type, event: &CGEvent| {
            if macos_event_should_trigger_selection(
                event_type,
                event.location(),
                event.get_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE),
                &tap_mouse_state,
            ) {
                let _ = tap_trigger_tx.send(());
            }

            // ListenOnly tap 不修改事件；返回 None 让系统继续传递原事件。
            None
        },
    )
    .map_err(|_| {
        GetTextError::fallback_failed(
            "create macOS CGEventTap; Accessibility permission may be missing",
        )
    })?;

    let run_loop = CFRunLoop::get_current();
    let source = tap
        .mach_port
        .create_runloop_source(0)
        .map_err(|_| GetTextError::fallback_failed("create macOS CGEventTap run loop source"))?;

    run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();

    let mut last_selection = None;
    while !stop_signal.load(Ordering::Acquire) {
        CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, interval, true);

        if drain_macos_trigger_events(&trigger_rx) && !stop_signal.load(Ordering::Acquire) {
            // 鼠标事件先于 AX 树稳定到达，短暂等待后复用 Pull API 读取。
            thread::sleep(interval);
            emit_current_selection(&opts, &mut last_selection, on_selection);
        }
    }

    run_loop.remove_source(&source, unsafe { kCFRunLoopCommonModes });

    Ok(())
}

/// 消耗当前积压的 macOS 触发事件；多个鼠标事件合并成一次读取。
fn drain_macos_trigger_events(trigger_rx: &Receiver<()>) -> bool {
    let mut has_trigger = false;
    loop {
        match trigger_rx.try_recv() {
            Ok(()) => has_trigger = true,
            Err(std::sync::mpsc::TryRecvError::Empty) => return has_trigger,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return has_trigger,
        }
    }
}

/// 将 macOS 鼠标事件归一化为“可能产生选区”的触发信号。
fn macos_event_should_trigger_selection(
    event_type: CGEventType,
    location: CGPoint,
    click_count: i64,
    mouse_state: &Mutex<MacosMouseTriggerState>,
) -> bool {
    let mut mouse_state = mouse_state.lock();
    match event_type {
        CGEventType::LeftMouseDown => {
            mouse_state.record_down(location);
            MacosMouseTriggerState::is_multi_click(click_count)
        }
        CGEventType::LeftMouseUp => mouse_state.finish_drag(location),
        _ => false,
    }
}

/// macOS 鼠标防抖状态机，负责识别拖拽选区和双击/三击选词。
#[derive(Debug)]
struct MacosMouseTriggerState {
    down_position: Option<CGPoint>,
    drag_threshold_px: f64,
}

impl MacosMouseTriggerState {
    /// 创建鼠标状态机，并规整异常阈值避免 NaN/负数影响判断。
    fn new(drag_threshold_px: f64) -> Self {
        Self {
            down_position: None,
            drag_threshold_px: sanitize_drag_threshold_px(drag_threshold_px),
        }
    }

    /// 记录鼠标按下坐标。
    fn record_down(&mut self, position: CGPoint) {
        self.down_position = Some(position);
    }

    /// 鼠标抬起时判断是否越过拖拽阈值。
    fn finish_drag(&mut self, up_position: CGPoint) -> bool {
        let Some(down_position) = self.down_position.take() else {
            return false;
        };

        moved_beyond_threshold(down_position, up_position, self.drag_threshold_px)
    }

    /// 双击/三击选词选段应直接触发读取。
    fn is_multi_click(click_count: i64) -> bool {
        click_count >= 2
    }
}

/// 规整鼠标拖拽阈值，避免异常配置导致触发器失效。
fn sanitize_drag_threshold_px(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

/// 使用平方距离判断拖拽是否越过阈值，避免在事件回调路径引入开方。
fn moved_beyond_threshold(down_position: CGPoint, up_position: CGPoint, threshold_px: f64) -> bool {
    let dx = up_position.x - down_position.x;
    let dy = up_position.y - down_position.y;
    let threshold = sanitize_drag_threshold_px(threshold_px);

    dx.mul_add(dx, dy * dy) > threshold * threshold
}

#[cfg(test)]
mod tests {
    use core_graphics::geometry::CGPoint;

    use super::{moved_beyond_threshold, sanitize_drag_threshold_px, MacosMouseTriggerState};

    #[test]
    fn macos_drag_distance_must_exceed_threshold() {
        assert!(!moved_beyond_threshold(
            CGPoint::new(0.0, 0.0),
            CGPoint::new(3.0, 0.0),
            3.0,
        ));
        assert!(moved_beyond_threshold(
            CGPoint::new(0.0, 0.0),
            CGPoint::new(3.1, 0.0),
            3.0,
        ));
    }

    #[test]
    fn macos_mouse_state_triggers_after_drag_threshold() {
        let mut state = MacosMouseTriggerState::new(3.0);
        state.record_down(CGPoint::new(0.0, 0.0));

        assert!(state.finish_drag(CGPoint::new(4.0, 0.0)));
    }

    #[test]
    fn macos_mouse_state_recognizes_multi_click_selection() {
        assert!(!MacosMouseTriggerState::is_multi_click(1));
        assert!(MacosMouseTriggerState::is_multi_click(2));
        assert!(MacosMouseTriggerState::is_multi_click(3));
    }

    #[test]
    fn macos_drag_threshold_sanitizes_invalid_values() {
        assert_eq!(sanitize_drag_threshold_px(-1.0), 0.0);
        assert_eq!(sanitize_drag_threshold_px(f64::NAN), 0.0);
        assert_eq!(sanitize_drag_threshold_px(4.0), 4.0);
    }
}
