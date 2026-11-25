use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
        Arc, OnceLock,
    },
    thread,
    time::Duration,
};

use parking_lot::Mutex;
use windows::Win32::{
    Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM},
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HHOOK, MSG,
        MSLLHOOKSTRUCT, PM_REMOVE, QS_ALLINPUT, WH_MOUSE_LL, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
        WM_LBUTTONUP,
    },
};

use crate::{DaemonOptions, GetTextError, SelectionResult};

use super::poll::{emit_current_selection, poll_interval_from_options};

/// 使用 Windows 低级鼠标 Hook 监听选区动作；不可用时返回错误交给轮询 fallback。
pub(super) fn run_windows_mouse_hook_loop(
    opts: DaemonOptions,
    stop_signal: Arc<AtomicBool>,
    on_selection: &impl Fn(SelectionResult),
) -> Result<(), GetTextError> {
    let interval = poll_interval_from_options(&opts);
    let wait_ms = duration_to_wait_ms(interval);
    let (trigger_tx, trigger_rx) = std::sync::mpsc::channel();
    let _hook = WindowsMouseHookGuard::install(opts.drag_threshold_px, trigger_tx)?;

    let mut last_selection = None;
    while !stop_signal.load(Ordering::Acquire) {
        pump_windows_hook_messages(wait_ms);

        if drain_windows_trigger_events(&trigger_rx) && !stop_signal.load(Ordering::Acquire) {
            // 鼠标消息先于 UIAutomation 树稳定到达，短暂等待后复用 Pull API。
            thread::sleep(interval);
            emit_current_selection(&opts, &mut last_selection, on_selection);
        }
    }

    Ok(())
}

/// WH_MOUSE_LL 依赖安装线程的消息循环；这里按 daemon 间隔做可停止等待。
fn pump_windows_hook_messages(wait_ms: u32) {
    unsafe {
        let _ = MsgWaitForMultipleObjects(None, false, wait_ms, QS_ALLINPUT);

        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// 消耗当前积压的 Windows 触发事件；多个鼠标事件合并成一次读取。
fn drain_windows_trigger_events(trigger_rx: &Receiver<()>) -> bool {
    let mut has_trigger = false;
    loop {
        match trigger_rx.try_recv() {
            Ok(()) => has_trigger = true,
            Err(std::sync::mpsc::TryRecvError::Empty) => return has_trigger,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return has_trigger,
        }
    }
}

/// 把 `Duration` 转成 Win32 等待毫秒，避免极端配置溢出。
fn duration_to_wait_ms(duration: Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
}

/// 安装/卸载 Windows 低级鼠标 Hook 的 RAII 句柄。
struct WindowsMouseHookGuard {
    hook: HHOOK,
}

impl WindowsMouseHookGuard {
    /// 安装全局低级鼠标 Hook，并注册回调用到的共享状态。
    fn install(drag_threshold_px: f64, trigger_tx: Sender<()>) -> Result<Self, GetTextError> {
        {
            let mut state = windows_mouse_hook_state().lock();
            if state.is_some() {
                return Err(GetTextError::fallback_failed(
                    "Windows mouse hook is already installed",
                ));
            }
            *state = Some(WindowsMouseHookState::new(drag_threshold_px, trigger_tx));
        }

        let hook = unsafe {
            SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(windows_low_level_mouse_proc),
                HINSTANCE::default(),
                0,
            )
        }
        .map_err(|error| {
            clear_windows_mouse_hook_state();
            GetTextError::fallback_failed(format!("install Windows WH_MOUSE_LL hook: {error}"))
        })?;

        Ok(Self { hook })
    }
}

impl Drop for WindowsMouseHookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.hook);
        }
        clear_windows_mouse_hook_state();
    }
}

/// Hook 回调必须极轻量：只判断鼠标动作并发送触发信号，绝不读取选区。
unsafe extern "system" fn windows_low_level_mouse_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if is_windows_hook_action(code) {
        let mouse = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if let Some(state) = windows_mouse_hook_state().lock().as_mut() {
            state.handle_mouse_message(wparam.0 as u32, mouse.pt);
        }
    }

    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}

/// Windows Hook 约定：只有 `HC_ACTION` 代表可处理的真实事件。
fn is_windows_hook_action(code: i32) -> bool {
    code == HC_ACTION as i32
}

/// 全局 Hook 没有 user_data，只能用进程内共享状态桥接到后台线程。
fn windows_mouse_hook_state() -> &'static Mutex<Option<WindowsMouseHookState>> {
    static STATE: OnceLock<Mutex<Option<WindowsMouseHookState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

/// 清理 Hook 回调共享状态。
fn clear_windows_mouse_hook_state() {
    *windows_mouse_hook_state().lock() = None;
}

/// Windows 鼠标防抖状态机，负责识别拖拽选区和双击选词。
#[derive(Debug)]
struct WindowsMouseHookState {
    down_position: Option<POINT>,
    drag_threshold_px: f64,
    trigger_tx: Sender<()>,
}

impl WindowsMouseHookState {
    /// 创建鼠标状态机，并规整异常阈值避免 NaN/负数影响判断。
    fn new(drag_threshold_px: f64, trigger_tx: Sender<()>) -> Self {
        Self {
            down_position: None,
            drag_threshold_px: sanitize_drag_threshold_px(drag_threshold_px),
            trigger_tx,
        }
    }

    /// 处理低级鼠标消息；达到选区动作条件时通知后台读取。
    fn handle_mouse_message(&mut self, message: u32, position: POINT) {
        if self.message_should_trigger_selection(message, position) {
            let _ = self.trigger_tx.send(());
        }
    }

    /// 判断 Windows 鼠标消息是否可能产生文本选区。
    fn message_should_trigger_selection(&mut self, message: u32, position: POINT) -> bool {
        match message {
            WM_LBUTTONDOWN => {
                self.down_position = Some(position);
                false
            }
            WM_LBUTTONUP => self.finish_drag(position),
            WM_LBUTTONDBLCLK => true,
            _ => false,
        }
    }

    /// 鼠标抬起时判断是否越过拖拽阈值。
    fn finish_drag(&mut self, up_position: POINT) -> bool {
        let Some(down_position) = self.down_position.take() else {
            return false;
        };

        moved_beyond_threshold(down_position, up_position, self.drag_threshold_px)
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

/// 使用平方距离判断拖拽是否越过阈值，避免在 Hook 回调路径引入开方。
fn moved_beyond_threshold(down_position: POINT, up_position: POINT, threshold_px: f64) -> bool {
    let dx = f64::from(up_position.x - down_position.x);
    let dy = f64::from(up_position.y - down_position.y);
    let threshold = sanitize_drag_threshold_px(threshold_px);

    dx.mul_add(dx, dy * dy) > threshold * threshold
}

#[cfg(test)]
mod tests {
    use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::WM_RBUTTONUP};

    use super::{
        duration_to_wait_ms, is_windows_hook_action, moved_beyond_threshold,
        sanitize_drag_threshold_px, WindowsMouseHookState, HC_ACTION, WM_LBUTTONDBLCLK,
        WM_LBUTTONDOWN, WM_LBUTTONUP,
    };

    #[test]
    fn windows_drag_distance_must_exceed_threshold() {
        assert!(!moved_beyond_threshold(
            POINT { x: 0, y: 0 },
            POINT { x: 3, y: 0 },
            3.0,
        ));
        assert!(moved_beyond_threshold(
            POINT { x: 0, y: 0 },
            POINT { x: 4, y: 0 },
            3.0,
        ));
    }

    #[test]
    fn windows_mouse_state_triggers_after_drag_threshold() {
        let (trigger_tx, _trigger_rx) = std::sync::mpsc::channel();
        let mut state = WindowsMouseHookState::new(3.0, trigger_tx);

        assert!(!state.message_should_trigger_selection(WM_LBUTTONDOWN, POINT { x: 0, y: 0 }));
        assert!(state.message_should_trigger_selection(WM_LBUTTONUP, POINT { x: 4, y: 0 }));
    }

    #[test]
    fn windows_mouse_state_recognizes_double_click_selection() {
        let (trigger_tx, _trigger_rx) = std::sync::mpsc::channel();
        let mut state = WindowsMouseHookState::new(3.0, trigger_tx);

        assert!(state.message_should_trigger_selection(WM_LBUTTONDBLCLK, POINT { x: 0, y: 0 },));
        assert!(!state.message_should_trigger_selection(WM_RBUTTONUP, POINT { x: 0, y: 0 }));
    }

    #[test]
    fn windows_drag_threshold_sanitizes_invalid_values() {
        assert_eq!(sanitize_drag_threshold_px(-1.0), 0.0);
        assert_eq!(sanitize_drag_threshold_px(f64::NAN), 0.0);
        assert_eq!(sanitize_drag_threshold_px(4.0), 4.0);
    }

    #[test]
    fn windows_hook_action_only_accepts_hc_action() {
        assert!(is_windows_hook_action(HC_ACTION as i32));
        assert!(!is_windows_hook_action(-1));
    }

    #[test]
    fn windows_wait_duration_saturates_to_u32() {
        assert_eq!(duration_to_wait_ms(Duration::from_millis(42)), 42);
        assert_eq!(
            duration_to_wait_ms(Duration::from_millis(u64::MAX)),
            u32::MAX,
        );
    }

    use std::time::Duration;
}
