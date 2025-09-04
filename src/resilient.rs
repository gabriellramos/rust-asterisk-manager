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

use crate::{Manager, ManagerOptions, AmiEvent, AmiError};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_stream::{Stream, StreamExt};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

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
}

impl Default for ResilientOptions {
    fn default() -> Self {
        Self {
            manager_options: ManagerOptions {
                port: 5038,
                host: "127.0.0.1".to_string(),
                username: "admin".to_string(),
                password: "password".to_string(),
                events: true,
            },
            buffer_size: 2048,
            enable_heartbeat: true,
            enable_watchdog: true,
            heartbeat_interval: 30,
            watchdog_interval: 1,
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
    manager.connect_and_login(options.manager_options.clone()).await?;
    
    // Start heartbeat if enabled
    if options.enable_heartbeat {
        manager.start_heartbeat().await?;
    }
    
    // Start watchdog if enabled  
    if options.enable_watchdog {
        manager.start_watchdog(options.manager_options).await?;
    }
    
    Ok(manager)
}

/// Create an infinite events stream that automatically handles reconnection and lag
/// 
/// This stream never ends on its own and will attempt to resubscribe to the 
/// broadcast channel on lag errors and recreate the stream on connection losses.
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
        let mut current_manager = manager;
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;
        
        loop {
            let mut event_stream = current_manager.all_events_stream().await;
            
            loop {
                match event_stream.next().await {
                    Some(Ok(event)) => {
                        // Reset retry count on successful event
                        retry_count = 0;
                        
                        // Check for internal connection lost events
                        if let AmiEvent::InternalConnectionLost { .. } = &event {
                            // Connection lost, break to outer loop to reconnect
                            yield Ok(event);
                            break;
                        } else {
                            yield Ok(event);
                        }
                    }
                    Some(Err(BroadcastStreamRecvError::Lagged(count))) => {
                        log::warn!("Event stream lagged by {} events, resubscribing", count);
                        // Stream lagged, resubscribe to get a fresh receiver
                        event_stream = current_manager.all_events_stream().await;
                        continue;
                    }
                    None => {
                        // Stream ended, try to reconnect
                        log::info!("Event stream ended, attempting reconnection");
                        break;
                    }
                }
            }
            
            // Attempt reconnection
            retry_count += 1;
            
            // Calculate retry delay with exponential backoff (capped at 30 seconds)
            let retry_delay = std::cmp::min(1u64 << (retry_count - 1), 30);
            
            if retry_count > MAX_RETRIES {
                log::error!("Max reconnection attempts reached, waiting {} seconds before retry", retry_delay);
                tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                retry_count = 0;
            }
            
            match connect_resilient(options.clone()).await {
                Ok(new_manager) => {
                    log::info!("Successfully reconnected to AMI");
                    current_manager = new_manager;
                    retry_count = 0;
                }
                Err(e) => {
                    log::warn!("Reconnection failed: {}, retrying in {} seconds", e, retry_delay);
                    tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                }
            }
        }
    }
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
        let deserialized: ResilientOptions = serde_json::from_str(&json).expect("Failed to deserialize");
        
        assert_eq!(opts.buffer_size, deserialized.buffer_size);
        assert_eq!(opts.enable_heartbeat, deserialized.enable_heartbeat);
        assert_eq!(opts.enable_watchdog, deserialized.enable_watchdog);
        assert_eq!(opts.heartbeat_interval, deserialized.heartbeat_interval);
        assert_eq!(opts.watchdog_interval, deserialized.watchdog_interval);
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
        };
        
        // Should fail to create due to connection failure
        let result = infinite_events_stream(opts).await;
        assert!(result.is_err());
    }
}