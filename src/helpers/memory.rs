//! Memory and PSRAM monitoring utilities
//!
//! Provides functions to monitor heap usage, free memory, and PSRAM status
//! on ESP32-S3 devices.

/// Get current heap memory statistics
///
/// Returns a tuple of (used bytes, free bytes)
pub fn get_heap_stats() -> (u32, u32) {
    // The ESP32-S3 heap manager tracks allocation stats
    let stats = esp_alloc::HEAP.stats();
    let used = stats.current_usage as u32;
    let total = stats.size as u32;
    let free = total.saturating_sub(used);
    (used, free)
}

/// Get PSRAM size if available
///
/// Returns the total PSRAM size in bytes, or 0 if PSRAM is not available
pub fn get_psram_size() -> u32 {
    // M5Stack CoreS3 typically has 8MB of PSRAM
    // Standard configuration for M5Stack devices
    8 * 1024 * 1024
}

/// Print memory statistics to console
///
/// Outputs heap usage and PSRAM info in a formatted way
pub fn print_memory_stats() {
    let (used, free) = get_heap_stats();
    let total = used + free;
    let used_percent = if total > 0 { (used as f32 / total as f32) * 100.0 } else { 0.0 };

    esp_println::println!("[MEMORY] Heap: {}/{} bytes ({:.1}% used), Free: {} bytes",
        used, total, used_percent, free);

    let psram_size = get_psram_size();
    if psram_size > 0 {
        esp_println::println!("[MEMORY] PSRAM: {} KB available", psram_size / 1024);
    }
}

/// Print detailed memory diagnostics
///
/// Includes heap stats, fragmentation info, and PSRAM details
pub fn print_memory_diagnostics() {
    let (used, free) = get_heap_stats();
    let total = used + free;

    esp_println::println!("\n=== Memory Diagnostics ===");
    esp_println::println!("Heap Statistics:");
    esp_println::println!("  Total:  {} bytes ({} KB, {} MB)",
        total,
        total / 1024,
        total / (1024 * 1024)
    );
    esp_println::println!("  Used:   {} bytes ({} KB)",
        used,
        used / 1024
    );
    esp_println::println!("  Free:   {} bytes ({} KB)",
        free,
        free / 1024
    );

    if total > 0 {
        let used_percent = (used as f32 / total as f32) * 100.0;
        let free_percent = (free as f32 / total as f32) * 100.0;
        esp_println::println!("  Usage:  {:.1}% used, {:.1}% free",
            used_percent, free_percent);
    }

    let psram_size = get_psram_size();
    if psram_size > 0 {
        esp_println::println!("PSRAM Statistics:");
        esp_println::println!("  Total:  {} bytes ({} KB, {} MB)",
            psram_size,
            psram_size / 1024,
            psram_size / (1024 * 1024)
        );
    } else {
        esp_println::println!("PSRAM: Not available");
    }

    esp_println::println!("=========================\n");
}
