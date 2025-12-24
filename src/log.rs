//! Simple logging macros using esp_println

/// Log an info message
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::__print_internal!("[INFO] {}", format_args!($($arg)*))
    };
}

/// Log a warning message
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::__print_internal!("[WARN] {}", format_args!($($arg)*))
    };
}

/// Log a debug message
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::__print_internal!("[DEBUG] {}", format_args!($($arg)*))
    };
}

// Internal macro for actual printing - uses esp_println
#[doc(hidden)]
#[macro_export]
macro_rules! __print_internal {
    ($fmt:expr, $arg:expr) => {
        esp_println::println!($fmt, $arg)
    };
}
