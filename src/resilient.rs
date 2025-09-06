//! Resilient AMI connection module
//!
//! This module provides resilient connection management for Asterisk AMI,
//! including automatic reconnection, heartbeat monitoring, and infinite event streams.
//!
//! # Example Usage
//!
//! ```rust,no_run
//! use asterisk_manager::resilient::{ResilientOptions, connect_resilient, infinite_events_stream};
//! use asterisk_manager::ManagerOptions;
//! use tokio_stream::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let resilient_options = ResilientOptions {
//!         manager_options: ManagerOptions {
//!             port: 5038,
//!             host: "127.0.0.1".to_string(),
//!             username: "admin".to_string(),
//!             password: "password".to_string(),
//!             events: true,
//!         },
//!         buffer_size: 2048,
//!         enable_heartbeat: true,
//!         enable_watchdog: true,
//!         heartbeat_interval: 30,
//!         watchdog_interval: 1,
//!         max_retries: 3,
//!         metrics: None,
//!         cumulative_attempts_counter: None,
//!     };
//!
//!     // Option 1: Connect with resilient features
//!     let manager = connect_resilient(resilient_options.clone()).await?;
//!     
//!     // Option 2: Create an infinite event stream that handles reconnection
//!     let mut event_stream = infinite_events_stream(resilient_options).await?;
//!     
//!     // Pin the stream for polling
//!     tokio::pin!(event_stream);
//!     
//!     while let Some(event_result) = event_stream.next().await {
//!         match event_result {
//!             Ok(event) => println!("Received event: {:?}", event),
//!             Err(e) => println!("Error receiving event: {:?}", e),
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```

use crate::{AmiError, AmiEvent, Manager, ManagerOptions};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{Stream, StreamExt};

/// Fixed delay in seconds between reconnection attempts after exceeding max_retries
const RECONNECTION_DELAY_SECONDS: u64 = 5;

/// Simple metrics for resilient connections (optional instrumentation)
#[derive(Debug, Clone, Default)]
pub struct ResilientMetrics {
    pub reconnection_attempts: Arc<AtomicU64>,
    pub successful_reconnections: Arc<AtomicU64>,
    pub failed_reconnections: Arc<AtomicU64>,
    pub connection_lost_events: Arc<AtomicU64>,
    pub last_reconnection_duration_ms: Arc<AtomicU64>,
}

impl ResilientMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_reconnection_attempt(&self) {
        self.reconnection_attempts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_successful_reconnection(&self, duration: Duration) {
        self.successful_reconnections
            .fetch_add(1, Ordering::Relaxed);
        self.last_reconnection_duration_ms
            .store(duration.as_millis() as u64, Ordering::Relaxed);
    }

    pub fn record_failed_reconnection(&self) {
        self.failed_reconnections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_connection_lost(&self) {
        self.connection_lost_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current metrics as a snapshot
    pub fn snapshot(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.reconnection_attempts.load(Ordering::Relaxed),
            self.successful_reconnections.load(Ordering::Relaxed),
            self.failed_reconnections.load(Ordering::Relaxed),
            self.connection_lost_events.load(Ordering::Relaxed),
            self.last_reconnection_duration_ms.load(Ordering::Relaxed),
        )
    }
}

/// Configuration options for resilient AMI connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilientOptions {
    /// Basic connection options
    pub manager_options: ManagerOptions,
    /// Buffer size for event broadcaster (default: 2048)
    pub buffer_size: usize,
    /// Enable heartbeat monitoring (default: true)
    pub enable_heartbeat: bool,
    /// Enable automatic reconnection watchdog (default: true)
    pub enable_watchdog: bool,
    /// Heartbeat interval in seconds (default: 30)
    pub heartbeat_interval: u64,
    /// Watchdog check interval in seconds (default: 1)
    pub watchdog_interval: u64,
    /// Maximum number of immediate reconnection attempts before adding delays.
    /// Retry behavior: first `max_retries` attempts are immediate (no delay),
    /// followed by one delayed attempt, then the counter resets and the cycle repeats.
    /// Default: 3.
    pub max_retries: u32,
    /// Optional metrics collection for monitoring reconnection behavior
    #[serde(skip)]
    pub metrics: Option<ResilientMetrics>,
    /// Global cumulative attempts counter shared across stream instances
    #[serde(skip)]
    pub cumulative_attempts_counter: Option<Arc<AtomicU64>>,
}

impl ResilientOptions {
    /// Create a new ResilientOptions with a shared global cumulative attempts counter.
    /// This ensures that cumulative attempt counts persist across stream recreation.
    pub fn with_global_counter(mut self) -> Self {
        self.cumulative_attempts_counter = Some(Arc::new(AtomicU64::new(0)));
        self
    }
}

impl Default for ResilientOptions {
    fn default() -> Self {
        Self {
            manager_options: ManagerOptions {
                port: 5038,
                host: "127.0.0.1".to_string(),
                username: "admin".to_string(),
                password: "admin".to_string(),
                events: true,
            },
            buffer_size: 2048,
            enable_heartbeat: true,
            enable_watchdog: true,
            heartbeat_interval: 30,
            watchdog_interval: 1,
            max_retries: 3,
            metrics: None,
            cumulative_attempts_counter: None,
        }
    }
}

/// Connect a manager with resilient features enabled
///
/// This function creates a Manager instance with the specified buffer size,
/// connects it, and optionally starts heartbeat and watchdog monitoring.
pub async fn connect_resilient(options: ResilientOptions) -> Result<Manager, AmiError> {
    let mut manager = Manager::new_with_buffer(options.buffer_size);

    // Connect and login
    manager
        .connect_and_login(options.manager_options.clone())
        .await?;

    // Start heartbeat if enabled
    if options.enable_heartbeat {
        manager
            .start_heartbeat_with_interval(options.heartbeat_interval)
            .await?;
    }

    // Start watchdog if enabled
    if options.enable_watchdog {
        manager
            .start_watchdog_with_interval(options.manager_options, options.watchdog_interval)
            .await?;
    }

    Ok(manager)
}

/// Create an infinite events stream that automatically handles reconnection and lag
///
/// Once successfully created, this stream never ends on its own and will attempt to
/// resubscribe to the broadcast channel on lag errors and recreate the stream on connection losses.
/// However, if the initial connection fails, the stream will not be created and an error will be returned.
pub async fn infinite_events_stream(
    options: ResilientOptions,
) -> Result<impl Stream<Item = Result<AmiEvent, AmiError>>, AmiError> {
    let manager = connect_resilient(options.clone()).await?;
    Ok(create_infinite_stream(manager, options))
}

fn create_infinite_stream(
    manager: Manager,
    options: ResilientOptions,
) -> impl Stream<Item = Result<AmiEvent, AmiError>> {
    async_stream::stream! {
        // Generate a unique ID for this stream instance to track lifecycle
        let stream_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        log::debug!("Creating new resilient stream instance [{}]", stream_id);

        let mut current_manager = manager;
        let mut retry_count = 0;

        // Use global counter if provided, otherwise create a local one
        let cumulative_counter = options.cumulative_attempts_counter
            .clone()
            .unwrap_or_else(|| Arc::new(AtomicU64::new(0)));

        loop {
            let mut event_stream = current_manager.all_events_stream().await;

            loop {
                match event_stream.next().await {
                    Some(Ok(event)) => {
                        // Reset retry count on successful event
                        retry_count = 0;

                        // Check for internal connection lost events
                        if let AmiEvent::InternalConnectionLost { .. } = &event {
                            // Record connection lost event in metrics if available
                            if let Some(ref metrics) = options.metrics {
                                metrics.record_connection_lost();
                            }
                            // Connection lost, break to outer loop to reconnect
                            yield Ok(event);
                            break;
                        } else {
                            yield Ok(event);
                        }
                    }
                    Some(Err(BroadcastStreamRecvError::Lagged(count))) => {
                        log::debug!("Event stream lagged by {} events, resubscribing", count);
                        // Stream lagged, resubscribe to get a fresh receiver
                        event_stream = current_manager.all_events_stream().await;
                        continue;
                    }
                    None => {
                        // Stream ended, try to reconnect
                        log::debug!("Event stream ended, attempting reconnection");
                        break;
                    }
                }
            }

            // Reconnection attempts with max_retries cycles
            'reconnect_loop: loop {
                // Start timing reconnection attempts
                let reconnection_start = Instant::now();

                // Record attempt in metrics if available
                if let Some(ref metrics) = options.metrics {
                    metrics.record_reconnection_attempt();
                }

                // Attempt reconnection
                retry_count += 1;
                let cumulative_attempts = cumulative_counter.fetch_add(1, Ordering::SeqCst) + 1;

                // Determine delay based on retry_count and max_retries
                let retry_delay = if retry_count <= options.max_retries {
                    0 // Immediate retry for the first max_retries attempts
                } else {
                    RECONNECTION_DELAY_SECONDS // Fixed delay after exceeding max_retries
                };

                // Log attempt info - use DEBUG for library internal logging
                log::debug!(
                    "[{}] Reconnection attempt #{}, cumulative #{}, delay {}s (max_retries={})",
                    stream_id,
                    retry_count,
                    cumulative_attempts,
                    retry_delay,
                    options.max_retries
                );

                // Try to reconnect and log result with attempt number
                log::debug!("[{}] Attempting to reconnect now (attempt #{}, cumulative #{})", stream_id, retry_count, cumulative_attempts);
                match connect_resilient(options.clone()).await {
                    Ok(new_manager) => {
                        let reconnection_duration = reconnection_start.elapsed();
                        // Record successful reconnection in metrics
                        if let Some(ref metrics) = options.metrics {
                            metrics.record_successful_reconnection(reconnection_duration);
                        }
                        log::debug!(
                            "[{}] Successfully reconnected to AMI on attempt #{}, cumulative #{} (total time since first failure: {:.1}s)",
                            stream_id,
                            retry_count,
                            cumulative_attempts,
                            reconnection_duration.as_secs_f64()
                        );
                        current_manager = new_manager;
                        retry_count = 0;
                        // Note: we don't reset cumulative_attempts here to track total attempts across all failures
                        break 'reconnect_loop; // Exit reconnection loop, go back to event streaming
                    }
                    Err(e) => {
                        // Record failed reconnection in metrics
                        if let Some(ref metrics) = options.metrics {
                            metrics.record_failed_reconnection();
                        }
                        log::debug!(
                            "[{}] Reconnection attempt #{} failed: {}, retrying in {} seconds",
                            stream_id,
                            retry_count,
                            e,
                            retry_delay
                        );

                        // Apply delay if needed
                        if retry_delay > 0 {
                            let sleep_start = std::time::Instant::now();
                            tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                            let sleep_duration = sleep_start.elapsed();
                            log::debug!(
                                "[{}] Retry delay completed after {:.1}s, starting next attempt...",
                                stream_id,
                                sleep_duration.as_secs_f64()
                            );
                        }

                        // Reset retry_count after going through a full cycle (max_retries immediate + 1 delayed attempt)
                        if retry_count > options.max_retries {
                            log::debug!(
                                "[{}] Completed retry cycle ({} immediate + 1 delayed), resetting counter",
                                stream_id,
                                options.max_retries
                            );
                            retry_count = 0;
                        }

                        // Continue the reconnection loop (will loop back to retry)
                        log::trace!("[{}] Continuing reconnection loop for next attempt", stream_id);
                    }
                }
            } // End of reconnection loop
        } // End of main event loop
    } // End of async_stream::stream!
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resilient_options_default() {
        let opts = ResilientOptions::default();
        assert_eq!(opts.buffer_size, 2048);
        assert!(opts.enable_heartbeat);
        assert!(opts.enable_watchdog);
        assert_eq!(opts.heartbeat_interval, 30);
        assert_eq!(opts.watchdog_interval, 1);
    }

    #[test]
    fn test_resilient_options_clone() {
        let opts = ResilientOptions {
            manager_options: ManagerOptions {
                port: 5038,
                host: "localhost".to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                events: true,
            },
            buffer_size: 1024,
            enable_heartbeat: false,
            enable_watchdog: false,
            heartbeat_interval: 60,
            watchdog_interval: 2,
            max_retries: 3,
            metrics: None,
            cumulative_attempts_counter: None,
        };
        let opts2 = opts.clone();
        assert_eq!(opts.buffer_size, opts2.buffer_size);
        assert_eq!(opts.enable_heartbeat, opts2.enable_heartbeat);
        assert_eq!(opts.enable_watchdog, opts2.enable_watchdog);
    }

    #[test]
    fn test_resilient_options_serialization() {
        let opts = ResilientOptions::default();

        // Test that it can be serialized and deserialized
        let json = serde_json::to_string(&opts).expect("Failed to serialize");
        let deserialized: ResilientOptions =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(opts.buffer_size, deserialized.buffer_size);
        assert_eq!(opts.enable_heartbeat, deserialized.enable_heartbeat);
        assert_eq!(opts.enable_watchdog, deserialized.enable_watchdog);
        assert_eq!(opts.heartbeat_interval, deserialized.heartbeat_interval);
        assert_eq!(opts.watchdog_interval, deserialized.watchdog_interval);
    }

    #[test]
    fn test_max_retries_behavior() {
        // Test that max_retries is properly included in ResilientOptions
        let opts = ResilientOptions {
            manager_options: ManagerOptions {
                port: 5038,
                host: "test".to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                events: true,
            },
            buffer_size: 1024,
            enable_heartbeat: false,
            enable_watchdog: false,
            heartbeat_interval: 30,
            watchdog_interval: 1,
            max_retries: 5, // Test custom value
            metrics: None,
            cumulative_attempts_counter: None,
        };

        assert_eq!(opts.max_retries, 5);

        // Test that default is 3
        let default_opts = ResilientOptions::default();
        assert_eq!(default_opts.max_retries, 3);
    }

    #[tokio::test]
    async fn test_connect_resilient_invalid_connection() {
        // Test connect_resilient with invalid connection parameters
        let opts = ResilientOptions {
            manager_options: ManagerOptions {
                port: 65534, // Invalid port
                host: "nonexistent.invalid".to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                events: true,
            },
            buffer_size: 1024,
            enable_heartbeat: false, // Disable to avoid heartbeat issues
            enable_watchdog: false,  // Disable to avoid watchdog reconnection
            heartbeat_interval: 30,
            watchdog_interval: 1,
            max_retries: 3,
            metrics: None,
            cumulative_attempts_counter: None,
        };

        // Should fail to connect
        let result = connect_resilient(opts).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_infinite_events_stream_creation() {
        // Test that infinite_events_stream can be created with valid options
        let opts = ResilientOptions {
            manager_options: ManagerOptions {
                port: 65535, // Invalid port to test error handling
                host: "nonexistent.invalid".to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                events: true,
            },
            buffer_size: 1024,
            enable_heartbeat: false,
            enable_watchdog: false,
            heartbeat_interval: 30,
            watchdog_interval: 1,
            max_retries: 3,
            metrics: None,
            cumulative_attempts_counter: None,
        };

        // Should fail to create due to connection failure
        let result = infinite_events_stream(opts).await;
        assert!(result.is_err());
    }
}
