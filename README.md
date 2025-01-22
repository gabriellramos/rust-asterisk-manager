# Asterisk Manager Library

This library provides an implementation to manage connections and authentication with an Asterisk Call Manager (AMI) server.

## Features

- **Reconnection Logic**: Automatically handles reconnections to the AMI server in case of connection drops.
- **Event Handling**: Processes various AMI events and provides a mechanism to handle them.
- **Asynchronous Operations**: Utilizes Tokio for non-blocking, asynchronous operations.
- **Participant Management**: Manages call participants and their states efficiently.
- **JSON Data Handling**: Formats and processes data into JSON objects for easy manipulation and integration.
- **Event Callbacks**: Allows registration of callbacks for specific events, all events, raw events, and response events.

## Usage Example

```rust,no_run
use asterisk_manager::{ManagerBuilder, ManagerOptions};
use tokio::runtime::Runtime;
use serde_json::Value;

fn main() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let options = ManagerOptions {
            port: 5038,
            host: "example.com".to_string(),
            username: "admin".to_string(),
            password: "password".to_string(),
            events: false,
        };

        let callback1 = Box::new(|event: Value| {
            println!("Callback1 received event: {}", event);
        });

        let callback2 = Box::new(|event: Value| {
            println!("Callback2 received event: {}", event);
        });

        let global_callback = Box::new(|event: Value| {
            println!("Global callback received event: {}", event);
        });

        let raw_event_callback = Box::new(|event: Value| {
            println!("Raw event callback received event: {}", event);
        });

        let response_event_callback = Box::new(|event: Value| {
            println!("Response event callback received event: {}", event);
        });

        let mut manager = ManagerBuilder::new(options)
            .on_event("Newchannel", callback1)
            .on_event("Hangup", callback2)
            .on_all_events(global_callback)
            .on_all_raw_events(raw_event_callback)
            .on_all_response_events(response_event_callback)
            .build();

        manager.connect_with_retries().await;

        if !manager.is_authenticated() {
            println!("Authentication failed");
            return;
        }

        let action = serde_json::json!({
            "action": "QueueStatus",
        });
        if let Err(err) = manager.send_action(action).await {
            println!("Error sending action: {}", err);
            return;
        }

        manager.read_data_with_retries().await;

        manager.disconnect().await;
    });
}
```

### Optional Parameters

The `event_sender` and `event_callback` parameters in the `Manager::new` method are optional. If you do not need to handle events or use callbacks, you can pass `None` for these parameters:

```rust
let mut manager = Manager::new(Some(options), None, None);
```

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
asterisk_manager = { path = "path/to/asterisk-manager" }
```

## Based On

This library is based on the [NodeJS-AsteriskManager](https://github.com/pipobscure/NodeJS-AsteriskManager) repository and the [node-asterisk](https://github.com/mscdex/node-asterisk) repository.
