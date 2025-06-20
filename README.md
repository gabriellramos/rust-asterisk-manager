# Asterisk Manager (asterisk-manager)

[](https://crates.io/crates/asterisk-manager)
[](https://docs.rs/asterisk-manager)
[](https://opensource.org/licenses/MIT)
[](https://github.com/gabriellramos/rust-asterisk-manager/actions/workflows/rust.yml)

A modern, asynchronous, strongly-typed, and stream-based library for interacting with the Asterisk Manager Interface (AMI) in Rust.

This crate simplifies communication with AMI by handling connection, authentication, sending actions, and consuming events in an idiomatic Rust way, using Tokio and a type system that helps prevent errors at compile time.

## Table of Contents

- [✨ Features](https://www.google.com/search?q=%23-features)
- [🚀 Getting Started](https://www.google.com/search?q=%23-getting-started)
  - [1. Installation](https://www.google.com/search?q=%231-installation)
  - [2. Usage Example](https://www.google.com/search?q=%232-usage-example)
- [📖 Core Concepts](https://www.google.com/search?q=%23-core-concepts)
  - [The `Manager`](https://www.google.com/search?q=%23the-manager)
  - [Sending Actions](https://www.google.com/search?q=%23sending-actions)
  - [Consuming Events](https://www.google.com/search?q=%23consuming-events)
  - [Error Handling](https://www.google.com/search?q=%23error-handling)
- [🔌 Reconnection Strategy](https://www.google.com/search?q=%23-reconnection-strategy)
- [🤝 Contributing](https://www.google.com/search?q=%23-contributing)
- [📜 License](https://www.google.com/search?q=%23-license)
- [⭐ Acknowledgements](https://www.google.com/search?q=%23-acknowledgements)

## ✨ Features

- **Strongly-Typed AMI Messages**: Actions, Events, and Responses are modeled as Rust `enum`s and `struct`s. This reduces runtime errors, improves safety, and enables powerful autocompletion in your editor.
- **Stream-Based API**: Consume AMI events reactively and efficiently using the `Stream` abstraction from `tokio_stream`, integrating seamlessly with the Tokio ecosystem.
- **Fully Asynchronous**: Built on Tokio for non-blocking, high-performance operations, ideal for concurrent applications.
- **Robust Concurrency**: The internal architecture uses dedicated I/O tasks (Reader, Writer, and Dispatcher) to prevent deadlocks, ensuring high performance even when receiving a flood of events while sending actions.
- **Action-Response Correlation**: Send an action and receive a `Future` that resolves to the corresponding response, making request/response logic straightforward.
- **Detailed Error Handling**: A comprehensive `AmiError` enum allows robust handling of different failure scenarios (I/O, authentication, parsing, timeouts, etc.).

## 🚀 Getting Started

### 1\. Installation

Add `asterisk-manager` to your `Cargo.toml`. The library requires Tokio as the async runtime.

```toml
[dependencies]
asterisk-manager = "1.0.0" # Replace with the latest version
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
log = "0.4"
```

*The dependencies `serde`, `serde_json`, and `uuid` are managed by `asterisk-manager`.*

### 2\. Usage Example

This example connects to AMI, listens for events in a separate task, sends a `Ping` action, and awaits the response.

```rust,no_run
use asterisk_manager::{Manager, ManagerOptions, AmiAction, AmiEvent};
use tokio_stream::StreamExt;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Define connection options
    let options = ManagerOptions {
        port: 5038,
        host: "127.0.0.1".to_string(),
        username: "admin".to_string(),
        password: "password".to_string(),
        events: true, // Enable receiving all events
    };

    // 2. Create a new, empty Manager instance
    let mut manager = Manager::new();

    // 3. Connect and login. This step now receives the options and starts the internal tasks.
    if let Err(e) = manager.connect_and_login(options).await {
        eprintln!("Failed to connect and login: {}", e);
        return Err(e.into());
    }
    println!("Connected and logged in to AMI!");

    // 4. Obtain a stream for all AMI events
    let mut event_stream = manager.all_events_stream().await;

    // 5. Start a task to continuously consume events
    tokio::spawn(async move {
        println!("Event task started. Waiting for events...");
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    // Handle the event
                    match event {
                        AmiEvent::PeerStatus(status) => {
                            println!("[Event] Peer Status: {} -> {}", status.peer, status.peer_status);
                        }
                        AmiEvent::Newchannel(new_channel) => {
                            println!("[Event] New channel created: {}", new_channel.channel);
                        }
                        _ => {
                            // Print other events
                            // println!("[Event] Received: {:?}", event);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Event stream error: {}", e);
                    // The stream breaking likely means the connection was lost.
                    break;
                }
            }
        }
        println!("Event task ended.");
    });

    // 6. Send an action and await the response
    println!("Sending Ping...");
    let ping_action = AmiAction::Ping { action_id: None };
    match manager.send_action(ping_action).await {
        Ok(response) => {
            println!("Ping response received: {:?}", response);
        }
        Err(e) => {
            eprintln!("Failed to send Ping action: {}", e);
        }
    }

    // Give some time for the event task to receive something (for this example only)
    tokio::time::sleep(Duration::from_secs(5)).await;

    // 7. Disconnect
    manager.disconnect().await?;
    println!("Disconnected.");

    Ok(())
}
```

## 📖 Core Concepts

### The `Manager`

The `Manager` struct is the main entry point of the library. It acts as a handle for the AMI connection. You first create an empty `Manager` with `Manager::new()` and then establish a connection with `manager.connect_and_login(options).await`. The `connect_and_login` method starts the internal I/O tasks that read and process all messages from the Asterisk server.

`Manager` is `Clone`, `Send`, and `Sync`, allowing it to be safely shared between multiple tasks, such as in a web application using Actix Web or Axum.

### Sending Actions

To send an action to Asterisk, use the `manager.send_action()` method. It accepts an instance of the `AmiAction` enum and returns a `Future`.

```rust
let action = AmiAction::Command {
    command: "sip show peers".to_string(),
    action_id: None, // The library generates a ActionID if None
};

let response_result = manager.send_action(action).await;
```

The `Future` resolves to a `Result<AmiResponse, AmiError>`. This allows you to asynchronously await the direct response to your action (e.g., `Response: Success` or `Response: Error`).

### Consuming Events

The library uses a "fan-out" approach for events. A single I/O task reads all events from Asterisk and broadcasts them to all interested consumers via a `broadcast::channel`.

To consume events, obtain a stream with `manager.all_events_stream().await`.

```rust
let mut stream = manager.all_events_stream().await;

while let Some(result) = stream.next().await {
    if let Ok(event) = result {
        println!("Event received: {:?}", event);
    }
}
```

This allows multiple parts of your application to independently and concurrently listen to the same AMI events without blocking each other.

### Error Handling

All fallible operations return `Result<T, AmiError>`. The `AmiError` enum describes the error source, allowing granular failure handling.

```rust
let mut manager = Manager::new();
match manager.connect_and_login(options).await {
    Ok(_) => println!("Success!"),
    Err(AmiError::Io(e)) => eprintln!("I/O error: {}", e),
    Err(AmiError::AuthenticationFailed(reason)) => eprintln!("Authentication failed: {}", reason),
    Err(AmiError::Timeout) => eprintln!("Operation timed out"),
    Err(e) => eprintln!("Other error: {}", e),
}
```

## 🔌 Reconnection Strategy

This library **does not** include built-in automatic reconnection logic by design. The philosophy is to provide robust building blocks so you can implement the reconnection strategy that best fits your application (e.g., exponential backoff, fixed number of attempts, etc.).

When the connection is lost, methods like `send_action` will return an `AmiError::ConnectionClosed`, and the event stream will end. Your application should handle these signals by creating a new `Manager` instance and calling `connect_and_login` again.

For a complete and robust example of a web application with resilient reconnection logic, see the [**`examples/actix_web_example.rs`**](https://www.google.com/search?q=examples/actix_web_example.rs) file in this repository.

## 🤝 Contributing

Contributions are very welcome\! If you find a bug, have a suggestion for improvement, or want to add support for more actions and events, feel free to open an [Issue](https://github.com/gabriellramos/rust-asterisk-manager/issues) or a [Pull Request](https://github.com/gabriellramos/rust-asterisk-manager/pulls).

## 📜 License

This project is licensed under the [MIT License](https://github.com/gabriellramos/rust-asterisk-manager/blob/master/LICENSE).

## ⭐ Acknowledgements

This work was inspired by the simplicity and effectiveness of the following libraries:

- [NodeJS-AsteriskManager](https://github.com/pipobscure/NodeJS-AsteriskManager)
- [node-asterisk](https://github.com/mscdex/node-asterisk)
- Thanks to all contributors and the Rust community.