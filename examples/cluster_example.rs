//! Example of using the Asterisk Cluster Manager
//!
//! This example demonstrates how to use the cluster manager to:
//! - Connect to multiple Asterisk servers
//! - Send actions to specific nodes or broadcast to all
//! - Handle unified event streams
//!
//! Note: This example requires running Asterisk servers to actually connect.
//! For testing purposes, it will attempt to connect but handle connection failures gracefully.

use asterisk_manager::cluster::AsteriskClusterManager;
use asterisk_manager::{ManagerOptions, AmiAction};
use tokio_stream::StreamExt;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Asterisk Cluster Manager Example");
    
    // Create a new cluster manager
    let cluster = AsteriskClusterManager::new();
    
    println!("📊 Initial cluster status:");
    println!("  Nodes: {}", cluster.node_count().await);
    
    // Configure multiple nodes (these would be real Asterisk servers in production)
    let configs = vec![
        ("asterisk-eu", ManagerOptions {
            host: "127.0.0.1".to_string(),
            port: 5038,
            username: "admin".to_string(),
            password: "secret".to_string(),
            events: true,
        }),
        ("asterisk-us", ManagerOptions {
            host: "127.0.0.1".to_string(),
            port: 5039, // Different port for simulation
            username: "admin".to_string(),
            password: "secret".to_string(),
            events: true,
        }),
    ];
    
    // Try to add nodes (will fail if no Asterisk servers are running, but that's OK for demo)
    println!("\n🔌 Attempting to add nodes to cluster...");
    for (node_id, config) in &configs {
        match cluster.add_node(*node_id, config.clone()).await {
            Ok(()) => println!("  ✅ Successfully added node: {}", node_id),
            Err(e) => println!("  ❌ Failed to add node {}: {} (this is expected if no server is running)", node_id, e),
        }
    }
    
    println!("\n📊 Cluster status after node addition:");
    println!("  Nodes: {}", cluster.node_count().await);
    println!("  Node IDs: {:?}", cluster.node_ids().await);
    
    // Show node connection status
    let status = cluster.node_status().await;
    println!("  Connection status:");
    for (node_id, connected) in &status {
        println!("    {}: {}", node_id, if *connected { "✅ Connected" } else { "❌ Disconnected" });
    }
    
    // Demonstrate sending actions to specific nodes
    println!("\n📤 Sending ping to specific node...");
    match cluster.send_to("asterisk-eu", AmiAction::Ping { action_id: Some("cluster-ping-1".to_string()) }).await {
        Ok(response) => println!("  ✅ Ping response from asterisk-eu: {:?}", response.response),
        Err(e) => println!("  ❌ Failed to ping asterisk-eu: {}", e),
    }
    
    // Demonstrate broadcasting to all nodes
    println!("\n📡 Broadcasting ping to all nodes...");
    let broadcast_results = cluster.broadcast(AmiAction::Ping { action_id: Some("cluster-broadcast-1".to_string()) }).await;
    for (node_id, result) in broadcast_results {
        match result {
            Ok(response) => println!("  ✅ Broadcast response from {}: {:?}", node_id, response.response),
            Err(e) => println!("  ❌ Broadcast failed to {}: {}", node_id, e),
        }
    }
    
    // Demonstrate filtered sending
    println!("\n🎯 Sending ping to nodes with 'eu' in the name...");
    let filtered_results = cluster.send_to_filtered(
        AmiAction::Ping { action_id: Some("cluster-filtered-1".to_string()) },
        |node_id, _node| node_id.contains("eu")
    ).await;
    for (node_id, result) in filtered_results {
        match result {
            Ok(response) => println!("  ✅ Filtered response from {}: {:?}", node_id, response.response),
            Err(e) => println!("  ❌ Filtered send failed to {}: {}", node_id, e),
        }
    }
    
    // Demonstrate event streaming (only if we have connected nodes)
    if cluster.node_count().await > 0 {
        println!("\n📺 Starting event stream (will run for 5 seconds)...");
        let mut event_stream = cluster.event_stream().await;
        
        // Use timeout to limit how long we listen for events
        let timeout_duration = Duration::from_secs(5);
        let start_time = std::time::Instant::now();
        
        while start_time.elapsed() < timeout_duration {
            match tokio::time::timeout(Duration::from_millis(500), event_stream.next()).await {
                Ok(Some(Ok(cluster_event))) => {
                    println!("  📨 Event from {}: {:?}", cluster_event.node_id, cluster_event.event);
                }
                Ok(Some(Err(e))) => {
                    println!("  ❌ Event error: {}", e);
                }
                Ok(None) => {
                    println!("  ℹ️  Event stream ended");
                    break;
                }
                Err(_) => {
                    // Timeout - no events received, continue
                }
            }
        }
        
        println!("  ⏰ Event listening timeout reached");
    } else {
        println!("\n📺 No connected nodes - skipping event stream demonstration");
    }
    
    // Demonstrate reconnection
    println!("\n🔄 Attempting to reconnect all nodes...");
    let reconnect_results = cluster.reconnect_all().await;
    for (node_id, result) in reconnect_results {
        match result {
            Ok(()) => println!("  ✅ Reconnected {}", node_id),
            Err(e) => println!("  ❌ Failed to reconnect {}: {}", node_id, e),
        }
    }
    
    // Clean shutdown
    println!("\n🛑 Shutting down cluster...");
    match cluster.shutdown().await {
        Ok(()) => println!("  ✅ Cluster shutdown successfully"),
        Err(errors) => {
            println!("  ⚠️  Cluster shutdown completed with {} errors:", errors.len());
            for error in errors {
                println!("    - {}", error);
            }
        }
    }
    
    println!("\n🎉 Cluster manager example completed!");
    println!("\nℹ️  Note: This example demonstrated the cluster API without requiring");
    println!("   actual Asterisk servers. In production, you would:");
    println!("   1. Configure real Asterisk AMI endpoints");
    println!("   2. Handle connection failures with retry logic");
    println!("   3. Implement proper error handling and logging");
    println!("   4. Use the event stream for real-time call monitoring");
    
    Ok(())
}