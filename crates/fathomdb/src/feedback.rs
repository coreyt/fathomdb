use std::collections::BTreeMap;
use std::fmt::Display;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use crate::new_row_id;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseCyclePhase {
    Started,
    Slow,
    Heartbeat,
    Finished,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseCycleEvent {
    pub operation_id: String,
    pub operation_kind: String,
    pub surface: String,
    pub phase: ResponseCyclePhase,
    pub elapsed_ms: u64,
    pub slow_threshold_ms: u64,
    pub metadata: BTreeMap<String, String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackConfig {
    pub slow_threshold_ms: u64,
    pub heartbeat_interval_ms: u64,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            slow_threshold_ms: 500,
            heartbeat_interval_ms: 2_000,
        }
    }
}

impl FeedbackConfig {
    #[must_use]
    pub fn new(slow_threshold_ms: u64, heartbeat_interval_ms: u64) -> Self {
        Self {
            slow_threshold_ms,
            heartbeat_interval_ms,
        }
    }
}

pub trait OperationObserver: Send + Sync {
    fn on_event(&self, event: &ResponseCycleEvent);
}

impl<F> OperationObserver for F
where
    F: Fn(&ResponseCycleEvent) + Send + Sync,
{
    fn on_event(&self, event: &ResponseCycleEvent) {
        self(event);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct OperationContext<'a> {
    pub surface: &'a str,
    pub operation_kind: &'a str,
}

struct SafeObserver<'a> {
    inner: &'a dyn OperationObserver,
    disabled: Arc<AtomicBool>,
}

impl<'a> SafeObserver<'a> {
    fn emit(&self, event: ResponseCycleEvent) {
        if self.disabled.load(Ordering::SeqCst) {
            return;
        }
        if catch_unwind(AssertUnwindSafe(|| self.inner.on_event(&event))).is_err() {
            self.disabled.store(true, Ordering::SeqCst);
        }
    }
}

pub(crate) fn run_with_feedback<T, E, F, C>(
    context: OperationContext<'_>,
    metadata: BTreeMap<String, String>,
    observer: Option<&dyn OperationObserver>,
    config: FeedbackConfig,
    error_code: C,
    operation: F,
) -> Result<T, E>
where
    E: Display,
    F: FnOnce() -> Result<T, E>,
    C: Fn(&E) -> Option<String>,
{
    let Some(observer) = observer else {
        return operation();
    };

    let operation_id = new_row_id();
    let started_at = Instant::now();
    let disabled = Arc::new(AtomicBool::new(false));
    let safe_observer = SafeObserver {
        inner: observer,
        disabled: Arc::clone(&disabled),
    };

    safe_observer.emit(build_event(
        &operation_id,
        context,
        ResponseCyclePhase::Started,
        0,
        config.slow_threshold_ms,
        metadata.clone(),
        None,
        None,
    ));

    std::thread::scope(|scope| {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let timer_observer = SafeObserver {
            inner: observer,
            disabled,
        };
        let timer_operation_id = operation_id.clone();
        let timer_metadata = metadata.clone();
        let timer = scope.spawn(move || {
            if stop_rx
                .recv_timeout(std::time::Duration::from_millis(config.slow_threshold_ms))
                .is_ok()
            {
                return;
            }
            timer_observer.emit(build_event(
                &timer_operation_id,
                context,
                ResponseCyclePhase::Slow,
                elapsed_ms(started_at),
                config.slow_threshold_ms,
                timer_metadata.clone(),
                None,
                None,
            ));
            loop {
                if stop_rx
                    .recv_timeout(std::time::Duration::from_millis(
                        config.heartbeat_interval_ms,
                    ))
                    .is_ok()
                {
                    return;
                }
                timer_observer.emit(build_event(
                    &timer_operation_id,
                    context,
                    ResponseCyclePhase::Heartbeat,
                    elapsed_ms(started_at),
                    config.slow_threshold_ms,
                    timer_metadata.clone(),
                    None,
                    None,
                ));
            }
        });

        let outcome = catch_unwind(AssertUnwindSafe(operation));
        let _ = stop_tx.send(());
        let _ = timer.join();

        match outcome {
            Ok(Ok(value)) => {
                safe_observer.emit(build_event(
                    &operation_id,
                    context,
                    ResponseCyclePhase::Finished,
                    elapsed_ms(started_at),
                    config.slow_threshold_ms,
                    metadata,
                    None,
                    None,
                ));
                Ok(value)
            }
            Ok(Err(error)) => {
                safe_observer.emit(build_event(
                    &operation_id,
                    context,
                    ResponseCyclePhase::Failed,
                    elapsed_ms(started_at),
                    config.slow_threshold_ms,
                    metadata,
                    error_code(&error),
                    Some(error.to_string()),
                ));
                Err(error)
            }
            Err(payload) => {
                safe_observer.emit(build_event(
                    &operation_id,
                    context,
                    ResponseCyclePhase::Failed,
                    elapsed_ms(started_at),
                    config.slow_threshold_ms,
                    metadata,
                    Some("panic".to_owned()),
                    Some("operation panicked".to_owned()),
                ));
                resume_unwind(payload);
            }
        }
    })
}

fn build_event(
    operation_id: &str,
    context: OperationContext<'_>,
    phase: ResponseCyclePhase,
    elapsed_ms: u64,
    slow_threshold_ms: u64,
    metadata: BTreeMap<String, String>,
    error_code: Option<String>,
    error_message: Option<String>,
) -> ResponseCycleEvent {
    ResponseCycleEvent {
        operation_id: operation_id.to_owned(),
        operation_kind: context.operation_kind.to_owned(),
        surface: context.surface.to_owned(),
        phase,
        elapsed_ms,
        slow_threshold_ms,
        metadata,
        error_code,
        error_message,
    }
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{
        FeedbackConfig, OperationContext, OperationObserver, ResponseCycleEvent,
        ResponseCyclePhase, run_with_feedback,
    };

    #[derive(Clone, Default)]
    struct RecordingObserver {
        events: Arc<Mutex<Vec<ResponseCycleEvent>>>,
    }

    impl RecordingObserver {
        fn phases(&self) -> Vec<ResponseCyclePhase> {
            self.events
                .lock()
                .expect("observer mutex")
                .iter()
                .map(|event| event.phase)
                .collect()
        }
    }

    impl OperationObserver for RecordingObserver {
        fn on_event(&self, event: &ResponseCycleEvent) {
            self.events
                .lock()
                .expect("observer mutex")
                .push(event.clone());
        }
    }

    #[test]
    fn slow_success_emits_started_slow_heartbeat_and_finished() {
        let observer = RecordingObserver::default();

        let result = run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "test.slow_success",
            },
            BTreeMap::new(),
            Some(&observer),
            FeedbackConfig::new(5, 10),
            |_| None,
            || {
                std::thread::sleep(Duration::from_millis(35));
                Ok::<_, std::io::Error>(())
            },
        );

        assert!(result.is_ok());
        let phases = observer.phases();
        assert_eq!(phases[0], ResponseCyclePhase::Started);
        assert!(phases.contains(&ResponseCyclePhase::Slow));
        assert!(phases.contains(&ResponseCyclePhase::Heartbeat));
        assert_eq!(phases.last(), Some(&ResponseCyclePhase::Finished));
    }

    #[test]
    fn failure_emits_single_terminal_event() {
        let observer = RecordingObserver::default();

        let result = run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "test.failure",
            },
            BTreeMap::new(),
            Some(&observer),
            FeedbackConfig::new(5, 10),
            |_| Some("io".to_owned()),
            || -> Result<(), std::io::Error> {
                std::thread::sleep(Duration::from_millis(15));
                Err(std::io::Error::other("boom"))
            },
        );

        assert!(result.is_err());
        let phases = observer.phases();
        assert_eq!(phases[0], ResponseCyclePhase::Started);
        assert_eq!(phases.last(), Some(&ResponseCyclePhase::Failed));
        assert_eq!(
            phases
                .iter()
                .filter(|phase| matches!(
                    phase,
                    ResponseCyclePhase::Finished | ResponseCyclePhase::Failed
                ))
                .count(),
            1
        );
    }
}
