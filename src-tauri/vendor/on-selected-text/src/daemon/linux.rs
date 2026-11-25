use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::{DaemonOptions, GetTextError, SelectionResult};

use super::poll::{emit_current_selection, poll_interval_from_options};

/// 使用 AT-SPI2 ObjectEvents 触发读取；事件源不可用时返回错误交给轮询 fallback。
pub(super) fn run_linux_atspi_event_loop(
    opts: DaemonOptions,
    stop_signal: Arc<AtomicBool>,
    on_selection: &impl Fn(SelectionResult),
) -> Result<(), GetTextError> {
    async_io::block_on(run_linux_atspi_event_loop_async(
        opts,
        stop_signal,
        on_selection,
    ))
}

async fn run_linux_atspi_event_loop_async(
    opts: DaemonOptions,
    stop_signal: Arc<AtomicBool>,
    on_selection: &impl Fn(SelectionResult),
) -> Result<(), GetTextError> {
    use atspi::{
        events::{Event, ObjectEvents},
        AccessibilityConnection,
    };
    use futures_lite::{future, StreamExt};

    enum EventPoll {
        Event(Box<Option<Result<Event, atspi::AtspiError>>>),
        Tick,
    }

    let atspi = AccessibilityConnection::new().await.map_err(|error| {
        GetTextError::fallback_failed(format!("connect AT-SPI2 events: {error}"))
    })?;
    atspi
        .register_event::<ObjectEvents>()
        .await
        .map_err(|error| {
            GetTextError::fallback_failed(format!("register AT-SPI2 object events: {error}"))
        })?;

    let interval = poll_interval_from_options(&opts);
    let events = atspi.event_stream();
    futures_lite::pin!(events);
    let mut last_selection = None;

    while !stop_signal.load(Ordering::Acquire) {
        let event_poll = future::race(
            async { EventPoll::Event(Box::new(events.next().await)) },
            async {
                async_io::Timer::after(interval).await;
                EventPoll::Tick
            },
        )
        .await;

        match event_poll {
            EventPoll::Event(event) => match *event {
                Some(Ok(event)) if is_linux_selection_trigger_event(&event) => {
                    async_io::Timer::after(interval).await;
                    emit_current_selection(&opts, &mut last_selection, on_selection);
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    return Err(GetTextError::fallback_failed(format!(
                        "read AT-SPI2 object event: {error}"
                    )));
                }
                None => {
                    return Err(GetTextError::fallback_failed("AT-SPI2 event stream ended"));
                }
            },
            EventPoll::Tick => {}
        }
    }

    Ok(())
}

/// 判断 AT-SPI2 事件是否可能影响当前文本选区。
fn is_linux_selection_trigger_event(event: &atspi::events::Event) -> bool {
    use atspi::events::{Event, ObjectEvents};

    matches!(
        event,
        Event::Object(
            ObjectEvents::SelectionChanged(_)
                | ObjectEvents::StateChanged(_)
                | ObjectEvents::TextBoundsChanged(_)
                | ObjectEvents::TextSelectionChanged(_)
                | ObjectEvents::TextChanged(_)
                | ObjectEvents::TextCaretMoved(_)
        )
    )
}

#[cfg(test)]
mod tests {
    use super::is_linux_selection_trigger_event;

    #[test]
    fn linux_selection_trigger_event_accepts_text_selection_changes() {
        use atspi::events::{object::TextSelectionChangedEvent, Event, ObjectEvents};

        let event = Event::Object(ObjectEvents::TextSelectionChanged(
            TextSelectionChangedEvent::default(),
        ));

        assert!(is_linux_selection_trigger_event(&event));
    }

    #[test]
    fn linux_selection_trigger_event_ignores_unrelated_object_events() {
        use atspi::events::{object::VisibleDataChangedEvent, Event, ObjectEvents};

        let event = Event::Object(ObjectEvents::VisibleDataChanged(
            VisibleDataChangedEvent::default(),
        ));

        assert!(!is_linux_selection_trigger_event(&event));
    }
}
