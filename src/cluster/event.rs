//! Cluster event types and error handling for multi-node AMI events.

use crate::{AmiEvent, AmiError};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// A wrapper around AmiEvent that includes node identification.
///
/// This allows consumers to know which Asterisk server generated the event
/// when working with multiple nodes in a cluster configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterEvent {
    /// The unique identifier of the node that generated this event
    pub node_id: String,
    /// The original AMI event from the node
    pub event: AmiEvent,
    /// Optional timestamp when the event was received by the cluster manager
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<std::time::SystemTime>,
}

impl ClusterEvent {
    /// Create a new cluster event with node identification
    pub fn new(node_id: String, event: AmiEvent) -> Self {
        Self {
            node_id,
            event,
            timestamp: Some(std::time::SystemTime::now()),
        }
    }

    /// Create a new cluster event without timestamp
    pub fn new_without_timestamp(node_id: String, event: AmiEvent) -> Self {
        Self {
            node_id,
            event,
            timestamp: None,
        }
    }
}

impl fmt::Display for ClusterEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {:?}", self.node_id, self.event)
    }
}

/// Errors that can occur when working with cluster events
#[derive(Debug, Error)]
pub enum ClusterEventError {
    /// Error from the underlying AMI connection
    #[error("AMI error from node '{node_id}': {error}")]
    AmiError {
        node_id: String,
        #[source]
        error: AmiError,
    },
    
    /// Error from the broadcast stream
    #[error("Broadcast stream error from node '{node_id}': {error}")]
    BroadcastError {
        node_id: String,
        #[source]
        error: BroadcastStreamRecvError,
    },
    
    /// Node not found error
    #[error("Node '{node_id}' not found in cluster")]
    NodeNotFound { node_id: String },
    
    /// No nodes available in cluster
    #[error("No nodes available in cluster")]
    NoNodesAvailable,
    
    /// Event stream closed
    #[error("Event stream closed for node '{node_id}'")]
    StreamClosed { node_id: String },
    
    /// Multiple errors from different nodes
    #[error("Multiple errors occurred: {errors:?}")]
    MultipleErrors { errors: Vec<ClusterEventError> },
}

impl ClusterEventError {
    /// Create a new AMI error for a specific node
    pub fn ami_error(node_id: String, error: AmiError) -> Self {
        Self::AmiError { node_id, error }
    }

    /// Create a new broadcast error for a specific node
    pub fn broadcast_error(node_id: String, error: BroadcastStreamRecvError) -> Self {
        Self::BroadcastError { node_id, error }
    }

    /// Create a new node not found error
    pub fn node_not_found(node_id: String) -> Self {
        Self::NodeNotFound { node_id }
    }

    /// Create a new stream closed error
    pub fn stream_closed(node_id: String) -> Self {
        Self::StreamClosed { node_id }
    }
}