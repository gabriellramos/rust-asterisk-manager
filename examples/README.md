# Actix Web Examples

This directory contains two Actix Web examples demonstrating different approaches to using the Asterisk Manager Interface (AMI) with this crate:

## `actix_web_example.rs` - Traditional Approach

The original example showing manual connection management with:
- Custom reconnection logic (`ensure_manager_connected` function)
- Manual retry handling with timeouts
- Custom event stream management with break/restart cycles
- 160+ lines of connection management code

## `actix_web_resilient_example.rs` - Resilient Approach

A modernized example using the resilient connection features:
- **Automatic reconnection**: No manual retry logic needed
- **Heartbeat monitoring**: Built-in connection health checks
- **Infinite event stream**: Never-ending stream that handles reconnection automatically
- **Simplified code**: ~80 fewer lines of boilerplate connection management
- **Production-ready**: Designed for 24/7 reliability

### Key Differences

#### Traditional Example Features:
```rust
// Manual reconnection function
async fn ensure_manager_connected(app_state: &AppState) -> Result<(), AmiError> {
    let mut attempts = 0;
    loop {
        // Manual retry logic with sleep delays
        // Custom error handling and timeouts
        // 30+ lines of connection management
    }
}

// Manual event stream with restart cycles
while let Some(event_result) = event_stream.next().await {
    match event_result {
        Err(e) => {
            error!("Stream error: {:?}. Restarting cycle.", e);
            break; // Break and restart the entire cycle
        }
        // ...
    }
}
```

#### Resilient Example Features:
```rust
// Simple resilient connection
let manager = connect_resilient(resilient_options.clone()).await?;

// Infinite stream that never breaks
let mut event_stream = infinite_events_stream(resilient_options).await?;
while let Some(event_result) = event_stream.next().await {
    match event_result {
        Err(e) => {
            // Errors are logged but stream continues - reconnection is automatic
            error!("Error in event stream (will auto-recover): {:?}", e);
        }
        // ...
    }
}
```

### Configuration Options

The resilient example supports configurable options:

```rust
let resilient_options = ResilientOptions {
    manager_options: ManagerOptions { /* AMI connection details */ },
    buffer_size: 2048,           // Larger buffer for high-throughput
    enable_heartbeat: true,      // Send ping every 30 seconds
    enable_watchdog: true,       // Auto-reconnect when disconnected
    heartbeat_interval: 30,      // Seconds between heartbeats
    watchdog_interval: 1,        // Seconds between reconnection attempts
};
```

### When to Use Each

- **Traditional (`actix_web_example.rs`)**: When you need full control over connection management or are integrating with existing manual retry systems
- **Resilient (`actix_web_resilient_example.rs`)**: For production applications requiring automatic recovery, minimal maintenance, and 24/7 reliability

The resilient approach is recommended for most production use cases as it significantly reduces code complexity while providing more robust connection handling.