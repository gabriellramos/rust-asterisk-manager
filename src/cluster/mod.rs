//! # Asterisk Cluster Manager
//!
//! This module provides support for managing multiple Asterisk AMI connections
//! simultaneously, enabling cluster-aware telecom applications.
//!
//! ## Features
//!
//! - Connect to multiple Asterisk servers concurrently
//! - Send actions to specific nodes or broadcast to all nodes
//! - Unified event stream with node identification
//! - Independent connection lifecycle management
//!
//! ## Example
//!
//! ```rust,no_run
//! use asterisk_manager::cluster::{AsteriskClusterManager, ClusterEvent};
//! use asterisk_manager::{ManagerOptions, AmiAction};
//! use tokio_stream::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut cluster = AsteriskClusterManager::new();
//!     
//!     // Add nodes
//!     let eu_config = ManagerOptions {
//!         host: "asterisk-eu.example.com".to_string(),
//!         port: 5038,
//!         username: "admin".to_string(),
//!         password: "secret".to_string(),
//!         events: true,
//!     };
//!     cluster.add_node("asterisk-eu", eu_config).await?;
//!     
//!     // Send to specific node
//!     cluster.send_to("asterisk-eu", AmiAction::Ping { action_id: None }).await?;
//!     
//!     // Unified event stream
//!     let mut events = cluster.event_stream().await;
//!     while let Some(event) = events.next().await {
//!         match event {
//!             Ok(cluster_event) => {
//!                 println!("[{}] Event: {:?}", cluster_event.node_id, cluster_event.event);
//!             }
//!             Err(e) => eprintln!("Event error: {}", e),
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```

pub mod event;
pub mod manager;
pub mod node;

#[cfg(test)]
mod tests;

pub use event::{ClusterEvent, ClusterEventError};
pub use manager::AsteriskClusterManager;
pub use node::NodeManager;