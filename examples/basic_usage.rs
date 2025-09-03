use asterisk_manager::{AmiAction, AmiEvent, Manager, ManagerOptions};
use std::env;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Get connection details from environment variables or use defaults
    let host = env::var("AMI_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("AMI_PORT")
        .unwrap_or_else(|_| "5038".to_string())
        .parse()
        .unwrap_or(5038);
    let username = env::var("AMI_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let password = env::var("AMI_PASSWORD").unwrap_or_else(|_| "password".to_string());

    let options = ManagerOptions {
        host,
        port,
        username,
        password,
        events: true,
    };

    println!("Connecting to AMI at {}:{}...", options.host, options.port);

    let mut manager = Manager::new();
    
    // Connect and login
    match manager.connect_and_login(options).await {
        Ok(()) => println!("✓ Successfully connected and authenticated!"),
        Err(e) => {
            eprintln!("✗ Failed to connect: {}", e);
            return Err(e.into());
        }
    }

    // Start listening to events in a background task
    let manager_clone = manager.clone();
    let event_task = tokio::spawn(async move {
        let mut stream = manager_clone.all_events_stream().await;
        let mut event_count = 0;
        
        while let Some(Ok(event)) = stream.next().await {
            event_count += 1;
            match event {
                AmiEvent::Newchannel(data) => {
                    println!("📞 New Channel: {} ({})", data.channel, data.uniqueid);
                }
                AmiEvent::Hangup(data) => {
                    println!("📵 Hangup: {} - Cause: {:?}", data.channel, data.cause_txt);
                }
                AmiEvent::BridgeEnter(data) => {
                    println!("🌉 Bridge Enter: {} joined bridge {}", data.channel, data.bridge_uniqueid);
                }
                AmiEvent::BridgeLeave(data) => {
                    println!("🌉 Bridge Leave: {} left bridge {}", data.channel, data.bridge_uniqueid);
                }
                AmiEvent::PeerStatus(data) => {
                    println!("👥 Peer Status: {} is {}", data.peer, data.peer_status);
                }
                AmiEvent::UnknownEvent { event_type, .. } => {
                    println!("❓ Unknown Event: {}", event_type);
                }
                AmiEvent::InternalConnectionLost { error } => {
                    println!("💔 Connection lost: {}", error);
                    break;
                }
            }
            
            // Stop after processing 10 events for this demo
            if event_count >= 10 {
                println!("📊 Processed {} events, stopping event listener...", event_count);
                break;
            }
        }
    });

    // Test various AMI actions
    println!("\n🏃 Testing AMI actions...");

    // 1. Health check
    match manager.health_check().await {
        Ok(true) => println!("✓ Health check: Connection is healthy"),
        Ok(false) => println!("⚠ Health check: Connection appears down"),
        Err(e) => println!("✗ Health check failed: {}", e),
    }

    // 2. Ping
    println!("\n📡 Sending Ping...");
    match manager.send_action(AmiAction::Ping { action_id: None }).await {
        Ok(response) => println!("✓ Ping response: {}", response.response),
        Err(e) => println!("✗ Ping failed: {}", e),
    }

    // 3. Status - get channel status
    println!("\n📋 Getting channel status...");
    match manager.send_action(AmiAction::Status { action_id: None }).await {
        Ok(response) => {
            if let Some(events) = response.fields.get("CollectedEvents") {
                let count = events.as_array().map_or(0, |a| a.len());
                println!("✓ Status response: {} channels found", count);
            } else {
                println!("✓ Status response: {}", response.response);
            }
        }
        Err(e) => println!("✗ Status failed: {}", e),
    }

    // 4. Example command (safely fails if dialplan doesn't exist)
    println!("\n⚡ Sending command...");
    let command_action = AmiAction::Command {
        command: "core show version".to_string(),
        action_id: None,
    };
    match manager.send_action(command_action).await {
        Ok(response) => println!("✓ Command response: {}", response.response),
        Err(e) => println!("✗ Command failed: {}", e),
    }

    // 5. Example originate (will likely fail in demo environment, but shows syntax)
    println!("\n📞 Testing originate action (may fail in demo environment)...");
    let originate_action = AmiAction::Originate {
        channel: "Local/demo@default".to_string(),
        context: "default".to_string(),
        exten: "echo".to_string(),
        priority: "1".to_string(),
        caller_id: Some("Demo <123>".to_string()),
        timeout: Some(10000),
        action_id: None,
    };
    match manager.send_action(originate_action).await {
        Ok(response) => println!("✓ Originate response: {}", response.response),
        Err(e) => println!("ℹ Originate failed (expected in demo): {}", e),
    }

    // Wait a bit for events to be processed
    println!("\n⏳ Waiting for events...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Clean shutdown
    println!("\n🔌 Disconnecting...");
    match manager.disconnect().await {
        Ok(()) => println!("✓ Disconnected successfully"),
        Err(e) => println!("⚠ Disconnect error: {}", e),
    }

    // Cancel the event task
    event_task.abort();

    println!("\n✨ Demo completed!");
    Ok(())
}