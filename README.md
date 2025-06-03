# Asterisk Manager Library (v1.0.0)

A modern, strongly-typed, async and stream-based library for interacting with the Asterisk Manager Interface (AMI).

## Features

- **Strongly-typed AMI messages**: Actions, Events and Responses as Rust enums/structs.
- **Stream-based API**: Consume events via `tokio_stream`.
- **Async operations**: Built on Tokio.
- **ActionID/Response correlation**: Each action can be tracked by its ActionID.

## Example

```rust,no_run
use asterisk_manager::{Manager, ManagerOptions, AmiAction};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let options = ManagerOptions {
        port: 5038,
        host: "127.0.0.1".to_string(),
        username: "admin".to_string(),
        password: "password".to_string(),
        events: true,
    };
    let manager = Manager::new(options);
    manager.connect_and_login().await.unwrap();
    let mut events = manager.all_events_stream().await;
    tokio::spawn(async move {
        while let Some(ev) = events.next().await {
            println!("Event: {:?}", ev);
        }
    });
    let resp = manager.send_action(AmiAction::Ping { action_id: None }).await.unwrap();
    println!("Ping response: {:?}", resp);
    manager.disconnect().await.unwrap();
}
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
asterisk-manager = "1.0.0"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1", features = ["v4"] }
log = "0.4"
```

## Notes

- Automatic reconnection is **not** handled by the library. If you need reconnection, implement it in your application logic (see the example in `examples/actix_web_example.rs`).
- Inspired by [NodeJS-AsteriskManager](https://github.com/pipobscure/NodeJS-AsteriskManager) and [node-asterisk](https://github.com/mscdex/node-asterisk), but focused on modern Rust and strong typing.
