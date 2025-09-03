//! Main cluster manager implementation for managing multiple Asterisk AMI connections.

use crate::{AmiAction, AmiResponse, AmiError, ManagerOptions};
use crate::cluster::{ClusterEvent, ClusterEventError, NodeManager};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::{Stream, StreamExt};
use tokio::sync::mpsc;

/// Main cluster manager for handling multiple Asterisk AMI connections
///
/// This manager allows you to:
/// - Add and remove nodes dynamically
/// - Send actions to specific nodes or broadcast to all nodes
/// - Receive a unified event stream from all nodes with node identification
/// - Handle node failures and reconnections independently
pub struct AsteriskClusterManager {
    /// Map of node ID to node manager
    nodes: Arc<Mutex<HashMap<String, NodeManager>>>,
}

impl AsteriskClusterManager {
    /// Create a new empty cluster manager
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a new node to the cluster
    ///
    /// # Arguments
    /// * `node_id` - Unique identifier for the node
    /// * `config` - Connection configuration for the node
    ///
    /// # Example
    /// ```rust,no_run
    /// use asterisk_manager::cluster::AsteriskClusterManager;
    /// use asterisk_manager::ManagerOptions;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut cluster = AsteriskClusterManager::new();
    /// let config = ManagerOptions {
    ///     host: "127.0.0.1".to_string(),
    ///     port: 5038,
    ///     username: "admin".to_string(),
    ///     password: "secret".to_string(),
    ///     events: true,
    /// };
    /// cluster.add_node("node1", config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add_node(&self, node_id: impl Into<String>, config: ManagerOptions) -> Result<(), AmiError> {
        let node_id = node_id.into();
        let mut node_manager = NodeManager::new(node_id.clone(), config);
        
        // Connect the node
        node_manager.connect().await?;
        
        // Add to the cluster
        let mut nodes = self.nodes.lock().await;
        nodes.insert(node_id, node_manager);
        
        Ok(())
    }

    /// Remove a node from the cluster
    ///
    /// # Arguments
    /// * `node_id` - Identifier of the node to remove
    pub async fn remove_node(&self, node_id: &str) -> Result<(), ClusterEventError> {
        let mut nodes = self.nodes.lock().await;
        
        if let Some(node) = nodes.remove(node_id) {
            // Disconnect the node gracefully
            let _ = node.disconnect().await;
            Ok(())
        } else {
            Err(ClusterEventError::node_not_found(node_id.to_string()))
        }
    }

    /// Send an action to a specific node
    ///
    /// # Arguments
    /// * `node_id` - Target node identifier
    /// * `action` - AMI action to send
    pub async fn send_to(&self, node_id: &str, action: AmiAction) -> Result<AmiResponse, ClusterEventError> {
        let nodes = self.nodes.lock().await;
        
        if let Some(node) = nodes.get(node_id) {
            node.send_action(action).await
                .map_err(|e| ClusterEventError::ami_error(node_id.to_string(), e))
        } else {
            Err(ClusterEventError::node_not_found(node_id.to_string()))
        }
    }

    /// Broadcast an action to all nodes in the cluster
    ///
    /// # Arguments
    /// * `action` - AMI action to broadcast
    ///
    /// # Returns
    /// Vector of results, one for each node. The order matches the order of node IDs.
    pub async fn broadcast(&self, action: AmiAction) -> Vec<(String, Result<AmiResponse, ClusterEventError>)> {
        let nodes = self.nodes.lock().await;
        let mut results = Vec::new();
        
        for (node_id, node) in nodes.iter() {
            let result = node.send_action(action.clone()).await
                .map_err(|e| ClusterEventError::ami_error(node_id.clone(), e));
            results.push((node_id.clone(), result));
        }
        
        results
    }

    /// Send an action to nodes matching a filter predicate
    ///
    /// # Arguments
    /// * `action` - AMI action to send
    /// * `filter` - Predicate function to select nodes
    pub async fn send_to_filtered<F>(&self, action: AmiAction, filter: F) -> Vec<(String, Result<AmiResponse, ClusterEventError>)>
    where
        F: Fn(&str, &NodeManager) -> bool,
    {
        let nodes = self.nodes.lock().await;
        let mut results = Vec::new();
        
        for (node_id, node) in nodes.iter() {
            if filter(node_id, node) {
                let result = node.send_action(action.clone()).await
                    .map_err(|e| ClusterEventError::ami_error(node_id.clone(), e));
                results.push((node_id.clone(), result));
            }
        }
        
        results
    }

    /// Get a unified event stream from all nodes in the cluster
    ///
    /// The returned stream will yield `ClusterEvent` instances that include
    /// the node ID along with the original AMI event.
    pub async fn event_stream(&self) -> impl Stream<Item = Result<ClusterEvent, ClusterEventError>> + Send + Unpin {
        let (tx, rx) = mpsc::channel(1024);
        let nodes = self.nodes.clone();
        
        // Spawn tasks to forward events from each node
        tokio::spawn(async move {
            let nodes_guard = nodes.lock().await;
            let mut handles = Vec::new();
            
            for (node_id, node) in nodes_guard.iter() {
                let node_id = node_id.clone();
                let mut event_stream = node.event_stream().await;
                let tx_clone = tx.clone();
                
                let handle = tokio::spawn(async move {
                    while let Some(result) = event_stream.next().await {
                        let cluster_result = match result {
                            Ok(event) => Ok(ClusterEvent::new(node_id.clone(), event)),
                            Err(e) => Err(ClusterEventError::broadcast_error(node_id.clone(), e)),
                        };
                        
                        if tx_clone.send(cluster_result).await.is_err() {
                            // Receiver closed
                            break;
                        }
                    }
                });
                
                handles.push(handle);
            }
            
            // Wait for all tasks to complete
            for handle in handles {
                let _ = handle.await;
            }
        });
        
        tokio_stream::wrappers::ReceiverStream::new(rx)
    }

    /// Get the list of node IDs currently in the cluster
    pub async fn node_ids(&self) -> Vec<String> {
        let nodes = self.nodes.lock().await;
        nodes.keys().cloned().collect()
    }

    /// Get the count of nodes in the cluster
    pub async fn node_count(&self) -> usize {
        let nodes = self.nodes.lock().await;
        nodes.len()
    }

    /// Check if a specific node exists in the cluster
    pub async fn has_node(&self, node_id: &str) -> bool {
        let nodes = self.nodes.lock().await;
        nodes.contains_key(node_id)
    }

    /// Get connection status for all nodes
    pub async fn node_status(&self) -> HashMap<String, bool> {
        let nodes = self.nodes.lock().await;
        let mut status = HashMap::new();
        
        for (node_id, node) in nodes.iter() {
            status.insert(node_id.clone(), node.is_connected().await);
        }
        
        status
    }

    /// Attempt to reconnect all disconnected nodes
    pub async fn reconnect_all(&self) -> HashMap<String, Result<(), AmiError>> {
        let mut nodes = self.nodes.lock().await;
        let mut results = HashMap::new();
        
        for (node_id, node) in nodes.iter_mut() {
            if !node.is_connected().await {
                let result = node.try_reconnect().await;
                results.insert(node_id.clone(), result);
            }
        }
        
        results
    }

    /// Disconnect all nodes and clear the cluster
    pub async fn shutdown(&self) -> Result<(), Vec<ClusterEventError>> {
        let mut nodes = self.nodes.lock().await;
        let mut errors = Vec::new();
        
        for (node_id, node) in nodes.iter() {
            if let Err(e) = node.disconnect().await {
                errors.push(ClusterEventError::ami_error(node_id.clone(), e));
            }
        }
        
        nodes.clear();
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for AsteriskClusterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AsteriskClusterManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsteriskClusterManager")
            .finish()
    }
}