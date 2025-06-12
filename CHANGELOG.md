# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


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