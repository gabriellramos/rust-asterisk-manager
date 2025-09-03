//! Node management utilities for cluster operations.

use crate::{Manager, ManagerOptions, AmiError, AmiAction, AmiResponse};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A wrapper around a Manager instance with additional cluster metadata
#[derive(Clone)]
pub struct NodeManager {
    /// Unique identifier for this node
    pub node_id: String,
    /// The underlying AMI manager instance
    pub manager: Manager,
    /// Configuration used to connect to this node
    pub config: ManagerOptions,
    /// Whether this node is currently connected and authenticated
    pub connected: Arc<Mutex<bool>>,
}

impl NodeManager {
    /// Create a new node manager with the given configuration
    pub fn new(node_id: String, config: ManagerOptions) -> Self {
        Self {
            node_id,
            manager: Manager::new(),
            config,
            connected: Arc::new(Mutex::new(false)),
        }
    }

    /// Connect and authenticate this node
    pub async fn connect(&mut self) -> Result<(), AmiError> {
        self.manager.connect_and_login(self.config.clone()).await?;
        *self.connected.lock().await = true;
        Ok(())
    }

    /// Disconnect this node
    pub async fn disconnect(&self) -> Result<(), AmiError> {
        let result = self.manager.disconnect().await;
        *self.connected.lock().await = false;
        result
    }

    /// Check if this node is currently connected
    pub async fn is_connected(&self) -> bool {
        *self.connected.lock().await && self.manager.is_authenticated().await
    }

    /// Send an action to this node
    pub async fn send_action(&self, action: AmiAction) -> Result<AmiResponse, AmiError> {
        if !self.is_connected().await {
            return Err(AmiError::NotConnected);
        }
        self.manager.send_action(action).await
    }

    /// Get the event stream for this node
    pub async fn event_stream(&self) -> impl tokio_stream::Stream<Item = Result<crate::AmiEvent, tokio_stream::wrappers::errors::BroadcastStreamRecvError>> + Send + Unpin {
        self.manager.all_events_stream().await
    }

    /// Try to reconnect this node if disconnected
    pub async fn try_reconnect(&mut self) -> Result<(), AmiError> {
        if self.is_connected().await {
            return Ok(());
        }

        // Create a new manager instance for clean reconnection
        self.manager = Manager::new();
        self.connect().await
    }
}

impl std::fmt::Debug for NodeManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeManager")
            .field("node_id", &self.node_id)
            .field("config", &format!("{}:{}", self.config.host, self.config.port))
            .finish()
    }
}