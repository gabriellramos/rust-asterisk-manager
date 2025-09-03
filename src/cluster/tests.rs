//! Tests for cluster functionality

#[cfg(test)]
mod cluster_tests {
    use crate::cluster::{AsteriskClusterManager, ClusterEvent, ClusterEventError, NodeManager};
    use crate::{ManagerOptions, AmiAction, AmiEvent};
    use std::time::Duration;
    use tokio_stream::StreamExt;

    fn create_test_config(port: u16) -> ManagerOptions {
        ManagerOptions {
            host: "127.0.0.1".to_string(),
            port,
            username: "test".to_string(),
            password: "test".to_string(),
            events: true,
        }
    }

    #[tokio::test]
    async fn test_cluster_manager_creation() {
        let cluster = AsteriskClusterManager::new();
        assert_eq!(cluster.node_count().await, 0);
        assert!(cluster.node_ids().await.is_empty());
    }

    #[tokio::test] 
    async fn test_cluster_event_creation() {
        let event = AmiEvent::UnknownEvent {
            event_type: "TestEvent".to_string(),
            fields: std::collections::HashMap::new(),
        };
        
        let cluster_event = ClusterEvent::new("test_node".to_string(), event.clone());
        assert_eq!(cluster_event.node_id, "test_node");
        assert!(cluster_event.timestamp.is_some());
        
        let cluster_event_no_ts = ClusterEvent::new_without_timestamp("test_node".to_string(), event);
        assert_eq!(cluster_event_no_ts.node_id, "test_node");
        assert!(cluster_event_no_ts.timestamp.is_none());
    }

    #[tokio::test]
    async fn test_node_manager_creation() {
        let config = create_test_config(5038);
        let node = NodeManager::new("test_node".to_string(), config.clone());
        
        assert_eq!(node.node_id, "test_node");
        assert_eq!(node.config.host, "127.0.0.1");
        assert_eq!(node.config.port, 5038);
        assert!(!node.is_connected().await);
    }

    #[tokio::test]
    async fn test_cluster_error_types() {
        use crate::AmiError;
        
        let ami_error = AmiError::NotConnected;
        let cluster_error = ClusterEventError::ami_error("test_node".to_string(), ami_error);
        
        match cluster_error {
            ClusterEventError::AmiError { node_id, .. } => {
                assert_eq!(node_id, "test_node");
            }
            _ => panic!("Expected AmiError variant"),
        }
        
        let not_found_error = ClusterEventError::node_not_found("missing_node".to_string());
        match not_found_error {
            ClusterEventError::NodeNotFound { node_id } => {
                assert_eq!(node_id, "missing_node");
            }
            _ => panic!("Expected NodeNotFound variant"),
        }
    }

    #[tokio::test]
    async fn test_cluster_manager_node_operations() {
        let cluster = AsteriskClusterManager::new();
        
        // Test node_ids and node_count with empty cluster
        assert_eq!(cluster.node_count().await, 0);
        assert!(cluster.node_ids().await.is_empty());
        assert!(!cluster.has_node("test_node").await);
        
        // Test attempting to send to non-existent node
        let ping_action = AmiAction::Ping { action_id: None };
        let result = cluster.send_to("non_existent", ping_action).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ClusterEventError::NodeNotFound { node_id } => {
                assert_eq!(node_id, "non_existent");
            }
            _ => panic!("Expected NodeNotFound error"),
        }
        
        // Test removing non-existent node
        let remove_result = cluster.remove_node("non_existent").await;
        assert!(remove_result.is_err());
    }

    #[tokio::test]
    async fn test_cluster_broadcast_empty() {
        let cluster = AsteriskClusterManager::new();
        let ping_action = AmiAction::Ping { action_id: None };
        
        let results = cluster.broadcast(ping_action).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_cluster_send_to_filtered_empty() {
        let cluster = AsteriskClusterManager::new();
        let ping_action = AmiAction::Ping { action_id: None };
        
        let results = cluster.send_to_filtered(ping_action, |_id, _node| true).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_cluster_shutdown_empty() {
        let cluster = AsteriskClusterManager::new();
        let result = cluster.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cluster_reconnect_all_empty() {
        let cluster = AsteriskClusterManager::new();
        let results = cluster.reconnect_all().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_cluster_event_stream_empty() {
        let cluster = AsteriskClusterManager::new();
        let mut stream = cluster.event_stream().await;
        
        // The stream should eventually close for an empty cluster
        // Use timeout to avoid waiting indefinitely
        let timeout_result = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;
        
        // The stream may be open initially but should close quickly since no nodes are spawning tasks
        // Let's just check that the stream either times out or returns None
        match timeout_result {
            Ok(Some(_)) => panic!("Empty cluster should not yield events"),
            Ok(None) => {}, // Stream ended, which is expected
            Err(_) => {}, // Timeout, which is also acceptable
        }
    }

    #[tokio::test]
    async fn test_node_status_empty() {
        let cluster = AsteriskClusterManager::new();
        let status = cluster.node_status().await;
        assert!(status.is_empty());
    }

    #[test]
    fn test_cluster_event_display() {
        let event = AmiEvent::UnknownEvent {
            event_type: "TestEvent".to_string(),
            fields: std::collections::HashMap::new(),
        };
        
        let cluster_event = ClusterEvent::new_without_timestamp("test_node".to_string(), event);
        let display_string = format!("{}", cluster_event);
        assert!(display_string.contains("test_node"));
        assert!(display_string.contains("TestEvent"));
    }

    #[test]
    fn test_cluster_manager_debug() {
        let cluster = AsteriskClusterManager::new();
        let debug_string = format!("{:?}", cluster);
        assert!(debug_string.contains("AsteriskClusterManager"));
    }

    #[test]
    fn test_node_manager_debug() {
        let config = create_test_config(5038);
        let node = NodeManager::new("test_node".to_string(), config);
        let debug_string = format!("{:?}", node);
        assert!(debug_string.contains("NodeManager"));
        assert!(debug_string.contains("test_node"));
        assert!(debug_string.contains("127.0.0.1:5038"));
    }

    #[test]
    fn test_cluster_manager_default() {
        let cluster1 = AsteriskClusterManager::new();
        let cluster2 = AsteriskClusterManager::default();
        
        // Both should be equivalent (we can't easily test this directly, but they should both be empty)
        let _ = (cluster1, cluster2);
    }
}