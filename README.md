# Asterisk Manager (asterisk-manager)

[![Crates.io](https://img.shields.io/crates/v/asterisk-manager.svg)](https://crates.io/crates/asterisk-manager)
[![Docs.rs](https://docs.rs/asterisk-manager/badge.svg)](https://docs.rs/asterisk-manager)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/gabriellramos/rust-asterisk-manager/actions/workflows/rust.yml/badge.svg)](https://github.com/gabriellramos/rust-asterisk-manager/actions/workflows/rust.yml)

A modern, asynchronous, strongly-typed, and stream-based library for interacting with the Asterisk Manager Interface (AMI) in Rust.

This crate simplifies communication with AMI by handling connection, authentication, sending actions, and consuming events in an idiomatic Rust way, using Tokio and a type system that helps prevent errors at compile time.

## Table of Contents

- [✨ Features](#-features)
- [🚀 Getting Started](#-getting-started)
  - [1. Installation](#1-installation)
  - [2. Usage Example](#2-usage-example)
- [📖 Core Concepts](#-core-concepts)
  - [The `Manager`](#the-manager)
  - [Sending Actions](#sending-actions)
  - [Consuming Events](#consuming-events)
- [🌐 Multi-Asterisk Cluster Support](#-multi-asterisk-cluster-support)
- [📝 API Documentation with Utoipa](#-api-documentation-with-utoipa)
- [🔌 Reconnection Strategy](#-reconnection-strategy)
- [🤝 Contributing](#-contributing)
- [📜 License](#-license)

## ✨ Features

-   **Strongly-Typed AMI Messages**: Actions, Events, and Responses are modeled as Rust `enum`s and `struct`s, improving type safety and enabling editor autocompletion.
-   **Multi-Asterisk Cluster Support**: Built-in support for managing multiple Asterisk AMI connections simultaneously with unified event streams and flexible action routing.
-   **Intelligent Action Handling**: The `send_action` method automatically detects actions that return lists of events (e.g., `PJSIPShowEndpoints`). It transparently collects all related events and bundles them into a single, aggregated `AmiResponse`.
-   **Robust Concurrency**: The internal architecture uses dedicated I/O tasks (Reader, Writer, and Dispatcher) to prevent deadlocks, ensuring high performance even when receiving a flood of events while sending actions.
-   **Stream-Based API**: Consume AMI events reactively and efficiently using the `Stream` abstraction from `tokio_stream`.
-   **Optional OpenAPI/Swagger Support**: Includes a `docs` feature flag to enable `utoipa::ToSchema` implementations on all public types, allowing for automatic and accurate API documentation generation.
-   **Detailed Error Handling**: A comprehensive `AmiError` enum allows robust handling of different failure scenarios.

## 🚀 Getting Started

### 1. Installation

Add `asterisk-manager` to your `Cargo.toml`.

```toml
[dependencies]
asterisk-manager = "2.0.0" # Or the latest version
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
```

To enable automatic API documentation support for frameworks like Actix-web or Axum, enable the `docs` feature:

```toml
[dependencies]
asterisk-manager = { version = "2.0.0", features = ["docs"] }
```

### 2. Usage Example

This example connects to AMI, sends a simple action (`Ping`), and then sends a list-based action (`PJSIPShowEndpoints`), demonstrating how `send_action` handles both transparently.

```rust,no_run
use asterisk_manager::{Manager, ManagerOptions, AmiAction};
use tokio_stream::StreamExt;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = ManagerOptions {
        port: 5038,
        host: "127.0.0.1".to_string(),
        username: "admin".to_string(),
        password: "password".to_string(),
        events: true,
    };

    let mut manager = Manager::new();
    manager.connect_and_login(options).await?;
    println!("Successfully connected to AMI!");

    // Send a simple action
    println!("\n--- Sending a simple action (Ping) ---");
    let ping_action = AmiAction::Ping { action_id: None };
    let ping_response = manager.send_action(ping_action).await?;
    println!("Ping Response: {:?}", ping_response);

    // Send a list-based action
    println!("\n--- Sending a list-based action (PJSIPShowEndpoints) ---");
    let list_action = AmiAction::Custom { 
        action: "PJSIPShowEndpoints".to_string(), 
        params: HashMap::new(),
        action_id: None 
    };
    let list_response = manager.send_action(list_action).await?;
    if let Some(events) = list_response.fields.get("CollectedEvents") {
        println!("Collected {} events.", events.as_array().map_or(0, |a| a.len()));
        // println!("Collected Events JSON: {}", events);
    }
    
    manager.disconnect().await?;
    println!("\nDisconnected.");
    Ok(())
}
```

## 📖 Core Concepts

### The `Manager`

The `Manager` struct is the main entry point. You first create an empty `Manager` with `Manager::new()` and then establish a connection with `manager.connect_and_login(options).await`. This method starts the internal, non-blocking I/O tasks. `Manager` is `Clone`, `Send`, and `Sync`, allowing it to be safely shared between multiple tasks.

### Sending Actions

Use the `manager.send_action()` method. It intelligently handles different types of responses:
-   For simple actions like `Ping`, it returns the direct response immediately.
-   For actions that return a list (e.g., `PJSIPShowEndpoints`), it automatically collects all related events, bundles them into a `CollectedEvents` field in the final `AmiResponse`, and returns after the list is complete.

### Consuming Events

The library uses a broadcast channel to fan-out events. To consume them, obtain a stream with `manager.all_events_stream().await`. This allows multiple parts of your application to independently listen to the same AMI events.

```rust,no_run
let mut stream = manager.all_events_stream().await;
while let Some(Ok(event)) = stream.next().await {
    println!("Event received: {:?}", event);
}
```


## 🌐 Multi-Asterisk Cluster Support

This library includes built-in support for managing multiple Asterisk AMI connections simultaneously through the `AsteriskClusterManager`. This is ideal for:

- High-availability VoIP systems
- Geo-distributed Asterisk deployments  
- Load-balanced call center infrastructure
- Multi-tenant hosted PBX platforms

### Basic Cluster Usage

```rust,no_run
use asterisk_manager::cluster::AsteriskClusterManager;
use asterisk_manager::{ManagerOptions, AmiAction};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cluster = AsteriskClusterManager::new();
    
    // Add nodes to the cluster
    let eu_config = ManagerOptions {
        host: "asterisk-eu.example.com".to_string(),
        port: 5038,
        username: "admin".to_string(),
        password: "secret".to_string(),
        events: true,
    };
    cluster.add_node("asterisk-eu", eu_config).await?;
    
    let us_config = ManagerOptions {
        host: "asterisk-us.example.com".to_string(),
        port: 5038,
        username: "admin".to_string(),
        password: "secret".to_string(),
        events: true,
    };
    cluster.add_node("asterisk-us", us_config).await?;
    
    // Send action to specific node
    cluster.send_to("asterisk-eu", AmiAction::Ping { action_id: None }).await?;
    
    // Broadcast action to all nodes
    let results = cluster.broadcast(AmiAction::Command { 
        command: "core show version".to_string(),
        action_id: None 
    }).await;
    
    // Unified event stream from all nodes
    let mut events = cluster.event_stream().await;
    while let Some(event) = events.next().await {
        match event {
            Ok(cluster_event) => {
                println!("[{}] Event: {:?}", cluster_event.node_id, cluster_event.event);
            }
            Err(e) => eprintln!("Event error: {}", e),
        }
    }
    
    Ok(())
}
```

### Cluster Features

- **Dynamic Node Management**: Add and remove nodes at runtime
- **Flexible Action Routing**: Send to specific nodes, broadcast to all, or filter by criteria
- **Unified Event Streams**: Receive events from all nodes with node identification
- **Independent Lifecycles**: Each node manages its own connection and reconnection
- **Health Monitoring**: Check connection status across all nodes
- **Graceful Shutdown**: Cleanly disconnect all nodes

For a complete example, see [`examples/cluster_example.rs`](https://github.com/gabriellramos/rust-asterisk-manager/blob/master/examples/cluster_example.rs).

## 📝 API Documentation with Utoipa

This library provides optional support for `utoipa` to automatically generate OpenAPI (Swagger) schemas for your web application.

To enable it, add the `docs` feature in your `Cargo.toml`:
```toml
asterisk-manager = { version = "2.0.0", features = ["docs"] }
```
With this feature enabled, all public types like `AmiResponse` and `AmiEvent` will derive `utoipa::ToSchema`. You can then reference them directly in your Actix-web or Axum controller documentation.

```rust,ignore
// In your application's controller
use asterisk_manager::AmiResponse;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    //...
    components(
        schemas(AmiResponse) // This now works directly!
    )
)]
pub struct ApiDoc;
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