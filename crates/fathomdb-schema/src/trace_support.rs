// Feature-gated tracing macros — expand to nothing when `tracing` is disabled.

macro_rules! trace_error {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::error!($($arg)*);
    };
}

macro_rules! trace_info {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::info!($($arg)*);
    };
}
