# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.1.1] - 2025-11-18

### Fixed

-   Fixed serialization of `AmiEvent::UnknownEvent` to the proper AMI shape, preserving the `Event` field and flattening all additional fields. Ensures stable JSON roundtrip. If your code relied on the previous (incorrect) representation, adjust your deserialization.

### Added

-   Traceability: per-`Manager` `instance_id` with standardized logs on creation, heartbeat, and watchdog.
-   Examples:
    -   `examples/watchdog_resilient_logging.rs`: minimal watchdog with logs.
    -   `examples/multiple_managers_logging.rs`: multiple Managers with `instance_id` in logs.
    -   `examples/roundtrip_demo.rs`: demonstrates corrected `UnknownEvent` roundtrip.
    -   `examples/INSTANCE_ID.md`: `instance_id` documentation.

### Changed

-   Standardized log messages using `{}` interpolation; improved message consistency.
-   Improved `serialize_ami_action` string interpolation.

---

## [2.1.0] - 2025-09-06

This release introduces a comprehensive resilient connection module for production-ready, high-availability applications. The new resilient module provides automatic reconnection, heartbeat monitoring, and fault-tolerant event streaming while maintaining full backward compatibility with existing APIs.

### Added

- **Resilient Connection Module**: Comprehensive solution for production-ready AMI connections with robust features:
    - `ResilientOptions` struct offering extensive configuration options:
        - `max_retries`: Configurable maximum retry attempts (default: 3)
        - `heartbeat_interval`: Configurable heartbeat interval in seconds (default: 30)
        - `watchdog_interval`: Configurable watchdog check interval (default: 1 second)
        - `buffer_size`: Configurable event broadcaster buffer size (default: 2048)
    - `connect_resilient()` function for creating resilient Manager instances
    - `infinite_events_stream()` for fault-tolerant, never-ending event streaming
    - **Heartbeat Monitoring**:
        - `start_heartbeat_with_interval()` method for custom heartbeat intervals
        - Backward-compatible `start_heartbeat()` method (uses 30-second default)
        - Watchdog monitoring for automatic reconnection on heartbeat failures
    - **Watchdog Functionality**: Monitors authentication status and triggers reconnection when needed
    - **Configurable Buffer Sizes**: Optimized memory usage for high-throughput applications via `Manager::new_with_buffer(size)`

- **Production-Ready Features**:
    - Fixed 5-second retry delays for predictable reconnection behavior
    - Comprehensive logging with timestamps and attempt tracking:
        - Production-appropriate log levels (DEBUG for periodic events, INFO/WARN for significant events)
        - Sleep duration verification to identify timing discrepancies
        - Stream lifecycle tracking with unique instance identifiers
    - Stream instance identification for debugging multi-instance scenarios
    - Global cumulative attempt counter that persists across stream recreations
    - Optional metrics collection with `ResilientMetrics` struct

- **New API Functions**:
    - `connect_resilient()` function for creating Manager instances with resilient features enabled
    - `infinite_events_stream()` for fault-tolerant, never-ending event streaming
    - `ResilientOptions` configuration struct for fine-tuning resilient behavior (heartbeat intervals, buffer sizes, etc.)

### Changed

-   **ResilientOptions additions**: The `ResilientOptions` struct now exposes additional configuration knobs:
    - `heartbeat_interval` (u64, seconds) — controls how often a heartbeat `Ping` is sent when `enable_heartbeat` is true (default: 30).
    - `max_retries` (u32) — number of immediate reconnection attempts before adding 5-second delays in cycles (default: 3).
    - `metrics` (Option<ResilientMetrics>) — optional metrics collection for monitoring reconnection behavior (default: None).

-   **Improved reconnection logging**: Enhanced visibility into reconnection attempts with detailed logs including:
    - Cumulative attempt counters and fixed 5-second delays
    - Sleep duration tracking to identify timing issues
    - Production-friendly log levels (periodic attempts use DEBUG, failures/successes use INFO/WARN)
    - Timestamps for predicted retry times

-   **Fixed delay implementation**: Uses fixed 5-second delays for predictable reconnection behavior and simplified troubleshooting.

Upgrade note: If your code constructs `ResilientOptions` manually, add the `max_retries` and `metrics` fields or switch to `ResilientOptions::default()` and adjust fields as needed. The default values preserve the previous behavior (30s heartbeat, 3 retries, no metrics).-   **Production-Ready Examples**:
    -   **Resilient Actix Web Example** (`examples/actix_web_resilient_example.rs`): Demonstrates resilient connections in a real-world web application scenario with automatic reconnection
    -   **Examples Documentation** (`examples/README.md`): Comprehensive comparison between traditional and resilient approaches

-   **Enhanced Documentation**: Updated README.md with detailed resilient features documentation, usage examples, and feature comparisons

### Fixed

-   Fixed compilation error in `examples/actix_web_example.rs` by removing incorrect `crate::` prefix

### Enhanced

-   **Testing**: Added 20 comprehensive unit tests covering all resilient functionality
-   **Backward Compatibility**: All existing APIs remain unchanged and fully functional

---

## [2.0.0] - 2025-06-20

This release introduces a complete architectural overhaul for robust concurrency and adds intelligent features for handling complex AMI actions. It includes breaking changes to the connection API and response structures.

### Added

-   **Intelligent List Collection**: The `send_action` method now automatically detects actions that return lists of events (e.g., `PJSIPShowEndpoints`). It transparently collects all related events and bundles them into a single, aggregated `AmiResponse` in a new `CollectedEvents` field.
-   **Optional API Documentation**: Added support for `utoipa` via a `docs` feature flag. When enabled, all public response and event types implement `ToSchema`, allowing consuming applications to easily generate OpenAPI/Swagger documentation.

### Changed

-   **BREAKING: Connection API**: The `Manager::new()` constructor no longer accepts `ManagerOptions`. The connection is now established via a new method `manager.connect_and_login(options)`, which requires a mutable `Manager` handle. This makes the connection lifecycle more explicit and robust.
-   **BREAKING: `AmiResponse` Structure**: The `fields` field in `AmiResponse` was changed from `HashMap<String, String>` to `HashMap<String, serde_json::Value>` to support embedding the complex list of collected events from list-based actions.

### Fixed

-   **Concurrency Deadlock**: Re-architected the internal I/O handling to use dedicated, non-blocking tasks (Reader, Writer, Dispatcher). This resolves fundamental deadlock and timeout issues that occurred in high-concurrency environments (like web servers with multiple workers), ensuring actions are processed quickly and reliably.

---

## [1.0.0] - 2025-06-12

This release marks a complete architectural overhaul of the library, moving from a callback-based design to a modern, stream-based, and strongly-typed API. It introduces significant breaking changes from the `0.1.x` series.

### Changed

-   **BREAKING: Architectural Redesign**: The library was rebuilt around an actor-like model. The `Manager` is now a lightweight, cloneable handle to an internal, thread-safe client that runs its own I/O task in the background.
-   **BREAKING: Event Handling**: The callback-based system (`.on_event()`, `.on_all_events()`, etc.) has been entirely replaced by a stream-based API. Event consumption is now done via `manager.all_events_stream()`, which returns a `tokio_stream::Stream`.
-   **BREAKING: Strongly-Typed Messages**: Replaced generic `serde_json::Value` for all messages with strongly-typed `AmiAction`, `AmiEvent`, and `AmiResponse` enums and structs. This improves type safety, reduces runtime errors, and provides better editor autocompletion.
-   **BREAKING: API Simplification**: The `ManagerBuilder` has been removed. The library now has a single entry point: `Manager::new(options)`.
-   **BREAKING: Action Submission**: The `send_action` method is now fully async and returns a `Future<Result<AmiResponse, AmiError>>`. It waits for the corresponding response from Asterisk instead of being a "fire-and-forget" method.
-   **BREAKING: Internal I/O Loop**: The user no longer needs to call `read_data_with_retries()`. The I/O loop is now an internal implementation detail and runs automatically in a background task after a successful `connect_and_login()`.
-   **BREAKING: Error Handling**: Replaced `Result<..., String>` with a comprehensive `AmiError` enum for structured error handling.

### Added

-   **Request/Response Correlation**: Built-in, automatic `ActionID` generation and correlation for all actions sent.
-   **Concurrent Event Consumption**: The new event stream system uses `tokio::sync::broadcast`, allowing multiple parts of an application to listen to events concurrently from a single AMI connection.
-   **New Example Applications**:
    -   A new "Getting Started" example demonstrating the stream-based API.
    -   A robust `actix_web_example.rs` showcasing a resilient web service with a self-healing event collection task and reconnection logic.

### Removed

-   Removed `ManagerBuilder`.
-   Removed all callback registration methods: `on_event`, `on_all_events`, `on_all_raw_events`, `on_all_response_events`.
-   Removed public I/O methods: `read_data`, `read_data_with_retries`.
-   Removed `Participant` management from the core library, allowing users to implement their own state management based on the event stream.

---

## [0.1.1] - 2025-01-22

Initial version of the library with a callback-based API.

### Added

-   `ManagerBuilder` for constructing a `Manager` instance with event callbacks.
-   Callback registration for specific, all, raw, and response events.
-   Internal reconnection logic with `connect_with_retries` and `read_data_with_retries`.
-   Basic participant management.
-   Used `serde_json::Value` for generic handling of actions and events.