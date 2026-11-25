use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use crate::{DaemonOptions, GetTextError, SelectionResult};

mod poll;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Push 模式守护进程句柄。
///
/// 句柄遵循 RAII：调用 [`DaemonHandle::stop`] 或直接 drop 都会请求后台线程退出，
/// 并等待线程完成当前触发周期。macOS / Windows / Linux 会优先使用平台原生事件源，
/// 其他平台或事件源不可用时回退到跨平台轮询。
pub struct DaemonHandle {
    stop_signal: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl DaemonHandle {
    /// 请求停止守护进程并等待后台线程退出。
    pub fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::Release);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// 返回当前句柄是否已发出停止信号。
    pub fn is_stopped(&self) -> bool {
        self.stop_signal.load(Ordering::Acquire)
    }
}

impl Drop for DaemonHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 启动后台选区守护进程。
///
/// 当前版本先提供稳定的 API 与可停止生命周期：macOS / Windows / Linux 会优先由平台
/// 原生事件源触发读取；其他平台或事件源不可用时，后台线程会按
/// [`DaemonOptions::trigger_delay_ms`] 的节奏轮询 Pull API。无论触发源如何，
/// 只有选中文本或坐标与上次回调结果不同时才触发 `on_selection`。
pub fn start_selection_daemon(
    opts: DaemonOptions,
    on_selection: impl Fn(SelectionResult) + Send + 'static,
) -> Result<DaemonHandle, GetTextError> {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let worker_stop_signal = Arc::clone(&stop_signal);
    let thread_handle = thread::Builder::new()
        .name("on-selected-text-daemon".into())
        .spawn(move || run_selection_daemon_loop(opts, worker_stop_signal, on_selection))
        .map_err(|error| {
            GetTextError::fallback_failed(format!("start selection daemon: {error}"))
        })?;

    Ok(DaemonHandle {
        stop_signal,
        thread_handle: Some(thread_handle),
    })
}

/// 运行当前平台最合适的 daemon 触发器；不可用时回退到轮询。
fn run_selection_daemon_loop(
    opts: DaemonOptions,
    stop_signal: Arc<AtomicBool>,
    on_selection: impl Fn(SelectionResult),
) {
    #[cfg(target_os = "macos")]
    {
        if macos::run_macos_event_tap_loop(opts.clone(), Arc::clone(&stop_signal), &on_selection)
            .is_ok()
        {
            return;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if linux::run_linux_atspi_event_loop(opts.clone(), Arc::clone(&stop_signal), &on_selection)
            .is_ok()
        {
            return;
        }
    }

    #[cfg(target_os = "windows")]
    {
        if windows::run_windows_mouse_hook_loop(
            opts.clone(),
            Arc::clone(&stop_signal),
            &on_selection,
        )
        .is_ok()
        {
            return;
        }
    }

    poll::run_selection_poll_loop(opts, stop_signal, on_selection);
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::Duration,
    };

    use super::DaemonHandle;

    #[test]
    fn daemon_handle_stop_sets_signal_and_joins_worker() {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let worker_stop_signal = Arc::clone(&stop_signal);
        let thread_handle = thread::spawn(move || {
            while !worker_stop_signal.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }
        });
        let mut handle = DaemonHandle {
            stop_signal,
            thread_handle: Some(thread_handle),
        };

        handle.stop();

        assert!(handle.is_stopped());
        assert!(handle.thread_handle.is_none());
    }
}
