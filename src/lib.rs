//! # Asterisk Manager Library
//!
//! This library provides an implementation to manage connections and authentication with an Asterisk Call Manager (AMI) server.
//!
//! It offers a robust and efficient way to interact with the AMI server, handling connection retries, authentication, and event processing seamlessly. The library is built using Tokio for asynchronous operations, ensuring high performance and scalability. Additionally, it processes and formats data into JSON objects for easy manipulation and integration.
//!
//! ## Features
//!
//! - **Reconnection Logic**: Automatically handles reconnections to the AMI server in case of connection drops.
//! - **Event Handling**: Processes various AMI events and provides a mechanism to handle them.
//! - **Asynchronous Operations**: Utilizes Tokio for non-blocking, asynchronous operations.
//! - **Participant Management**: Manages call participants and their states efficiently.
//! - **JSON Data Handling**: Formats and processes data into JSON objects for easy manipulation.
//! - **Event Callbacks**: Allows registration of callbacks for specific events, all events, raw events, and response events.
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use asterisk_manager::{ManagerBuilder, ManagerOptions};
//! use tokio::runtime::Runtime;
//! use serde_json::Value;
//!
//! fn main() {
//!     let rt = Runtime::new().unwrap();
//!     rt.block_on(async {
//!         let options = ManagerOptions {
//!             port: 5038,
//!             host: "example.com".to_string(),
//!             username: "admin".to_string(),
//!             password: "password".to_string(),
//!             events: false,
//!         };
//!
//!         let callback1 = Box::new(|event: Value| {
//!             println!("Callback1 received event: {}", event);
//!         });
//!
//!         let callback2 = Box::new(|event: Value| {
//!             println!("Callback2 received event: {}", event);
//!         });
//!
//!         let global_callback = Box::new(|event: Value| {
//!             println!("Global callback received event: {}", event);
//!         });
//!
//!         let raw_event_callback = Box::new(|event: Value| {
//!             println!("Raw event callback received event: {}", event);
//!         });
//!
//!         let response_event_callback = Box::new(|event: Value| {
//!             println!("Response event callback received event: {}", event);
//!         });
//!
//!         let mut manager = ManagerBuilder::new(options)
//!             .on_event("Newchannel", callback1)
//!             .on_event("Hangup", callback2)
//!             .on_all_events(global_callback)
//!             .on_all_raw_events(raw_event_callback)
//!             .on_all_response_events(response_event_callback)
//!             .build();
//!
//!         manager.connect_with_retries().await;
//!
//!         if !manager.is_authenticated() {
//!             println!("Authentication failed");
//!             return;
//!         }
//!
//!         let action = serde_json::json!({
//!             "action": "QueueStatus",
//!         });
//!         if let Err(err) = manager.send_action(action).await {
//!             println!("Error sending action: {}", err);
//!             return;
//!         }
//!
//!         manager.read_data_with_retries().await;
//!
//!         manager.disconnect().await;
//!     });
//! }
//! ```
//!
//! ## Participant Structure
//!
//! The `Participant` structure represents a participant in a call.
//!
//! ```rust
//! #[derive(Debug)]
//! pub struct Participant {
//!     name: String,
//!     number: String,
//!     with: Option<String>,
//! }
//! ```
//!
//! ## Manager Structure
//!
//! The `Manager` structure handles connections and options for the AMI server.
//!
//! ```rust
//! use asterisk_manager::ManagerOptions;
//! use tokio::net::TcpStream;
//! use std::collections::HashMap;
//! use tokio::sync::mpsc;
//! use asterisk_manager::Participant;
//! use std::sync::{Arc, Mutex};
//!
//! pub struct Manager {
//!     options: ManagerOptions,
//!     connection: Option<TcpStream>,
//!     authenticated: bool,
//!     emitter: Arc<Mutex<Vec<String>>>,
//!     event_sender: Option<mpsc::Sender<String>>,
//!     participants: HashMap<String, Participant>,
//!     event_callbacks: HashMap<String, Box<dyn Fn(serde_json::Value) + Send + Sync>>,
//!     global_callback: Option<Box<dyn Fn(serde_json::Value) + Send + Sync>>,
//!     raw_event_callback: Option<Box<dyn Fn(serde_json::Value) + Send + Sync>>,
//!     response_event_callback: Option<Box<dyn Fn(serde_json::Value) + Send + Sync>>,
//! }
//! ```
//!
//! ## Main Methods
//!
//! ### `Manager::new`
//! Creates a new instance of the Manager. The `event_sender` and `event_callback` parameters are optional.
//!
//! ### `Manager::connect`
//! Connects to the AMI server.
//!
//! ### `Manager::connect_with_retries`
//! Connects to the AMI server with reconnection logic.
//!
//! ### `Manager::authenticate`
//! Authenticates with the AMI server.
//!
//! ### `Manager::send_action`
//! Sends an action to the AMI server.
//!
//! ### `Manager::read_data`
//! Reads data from the AMI server.
//!
//! ### `Manager::read_data_with_retries`
//! Reads data from the AMI server with reconnection logic.
//!
//! ### `Manager::on_event`
//! Processes an event received from the AMI server.
//!
//! ### `Manager::parse_event`
//! Parses an event received from the AMI server.
//!
//! ### `Manager::disconnect`
//! Disconnects from the AMI server.
//!
//! ### `Manager::is_authenticated`
//! Returns whether the manager is authenticated.
//!
//! ## ManagerBuilder Structure
//!
//! The `ManagerBuilder` structure helps in building a `Manager` instance with various event callbacks.
//!
//! ### `ManagerBuilder::new`
//! Creates a new instance of the `ManagerBuilder`.
//!
//! ### `ManagerBuilder::on_event`
//! Registers a callback for a specific event.
//!
//! ### `ManagerBuilder::on_all_events`
//! Registers a callback for all events.
//!
//! ### `ManagerBuilder::on_all_raw_events`
//! Registers a callback for all raw events.
//!
//! ### `ManagerBuilder::on_all_response_events`
//! Registers a callback for all response events.
//!
//! ### `ManagerBuilder::build`
//! Builds and returns a `Manager` instance.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

/// Structure that contains the Manager configuration options.
/// Represents the configuration options for the manager.
///
/// # Fields
///
/// * `port` - The port to connect to Asterisk AMI.
/// * `host` - The host to connect to Asterisk AMI.
/// * `username` - The username to authenticate with.
/// * `password` - The password to authenticate with.
/// * `events` - Indicates whether events should be enabled.
///
/// # Example
///
/// ```rust
/// use asterisk_manager::ManagerOptions;
///
/// let options = ManagerOptions {
///     port: 5038,
///     host: "example.com".to_string(),
///     username: "admin".to_string(),
///     password: "password".to_string(),
///     events: false,
/// };
/// ```
///
/// # Example (JSON)
///
/// ```json
/// {
///     "port": 5038,
///     "host": "example.com",
///     "username": "admin",
///     "password": "password",
///     "events": false
/// }
/// ```
///
#[derive(Serialize, Deserialize, Debug)]
pub struct ManagerOptions {
    pub port: u16,
    pub host: String,
    pub username: String,
    pub password: String,
    pub events: bool,
}

/// Structure that represents the Manager.
/// Represents a manager that handles connections and options.
///
/// # Fields
///
/// * `options` - Configuration options for the manager.
/// * `connection` - An optional TCP stream representing the connection.
/// * `authenticated` - Indicates whether the manager is authenticated. This field is currently unused.
/// * `emitter` - A thread-safe vector of strings used for emitting events or messages.
pub struct Manager {
    options: ManagerOptions,
    connection: Option<TcpStream>,
    authenticated: bool,
    emitter: Arc<Mutex<Vec<String>>>,
    event_sender: Option<mpsc::Sender<String>>,
    participants: HashMap<String, Participant>,
    event_callbacks: HashMap<String, Box<dyn Fn(Value) + Send + Sync>>,
    global_callback: Option<Box<dyn Fn(Value) + Send + Sync>>,
    raw_event_callback: Option<Box<dyn Fn(Value) + Send + Sync>>,
    response_event_callback: Option<Box<dyn Fn(Value) + Send + Sync>>,
}

pub struct ManagerBuilder {
    options: ManagerOptions,
    #[allow(dead_code)]
    event_senders: HashMap<String, mpsc::Sender<String>>,
    event_callbacks: HashMap<String, Box<dyn Fn(Value) + Send + Sync>>,
    global_callback: Option<Box<dyn Fn(Value) + Send + Sync>>,
    raw_event_callback: Option<Box<dyn Fn(Value) + Send + Sync>>,
    response_event_callback: Option<Box<dyn Fn(Value) + Send + Sync>>,
}

/// A builder for creating a `Manager` instance with various event callbacks.
///
/// # Example
///
/// ```
/// let manager = ManagerBuilder::new(options)
///     .on_event("event_name", Box::new(|value| {
///         // handle event
///     }))
///     .on_all_events(Box::new(|value| {
///         // handle all events
///     }))
///     .on_all_raw_events(Box::new(|value| {
///         // handle all raw events
///     }))
///     .on_all_response_events(Box::new(|value| {
///         // handle all response events
///     }))
///     .build();
/// ```
///
/// # Methods
///
/// - `new(options: ManagerOptions) -> Self`: Creates a new `ManagerBuilder` with the specified options.
/// - `on_event(mut self, event: &str, callback: Box<dyn Fn(Value) + Send + Sync>) -> Self`: Registers a callback for a specific event.
/// - `on_all_events(mut self, callback: Box<dyn Fn(Value) + Send + Sync>) -> Self`: Registers a callback for all events.
/// - `on_all_raw_events(mut self, callback: Box<dyn Fn(Value) + Send + Sync>) -> Self`: Registers a callback for all raw events.
/// - `on_all_response_events(mut self, callback: Box<dyn Fn(Value) + Send + Sync>) -> Self`: Registers a callback for all response events.
/// - `build(self) -> Manager`: Consumes the builder and returns a `Manager` instance.
impl ManagerBuilder {
    /// Creates a new instance of the `ManagerBuilder`.
    /// Initializes the builder with the specified options.
    pub fn new(options: ManagerOptions) -> Self {
        ManagerBuilder {
            options,
            event_senders: HashMap::new(),
            event_callbacks: HashMap::new(),
            global_callback: None,
            raw_event_callback: None,
            response_event_callback: None,
        }
    }

    /// Registers a callback for a specific event.
    pub fn on_event(mut self, event: &str, callback: Box<dyn Fn(Value) + Send + Sync>) -> Self {
        self.event_callbacks.insert(event.to_string(), callback);
        self
    }

    /// Registers a callback for all events.
    pub fn on_all_events(mut self, callback: Box<dyn Fn(Value) + Send + Sync>) -> Self {
        self.global_callback = Some(callback);
        self
    }

    /// Registers a callback for all raw events.
    ///
    /// # Example
    ///
    /// ```
    /// let manager = ManagerBuilder::new(options)
    ///    .on_all_response_events(Box::new(|value| {
    ///       println!("Raw event received: {}", value);
    ///   }))
    ///  .build();
    /// ```
    ///
    /// # Arguments
    ///
    /// * `callback` - A callback function that receives a `Value` object.
    pub fn on_all_raw_events(mut self, callback: Box<dyn Fn(Value) + Send + Sync>) -> Self {
        self.raw_event_callback = Some(callback);
        self
    }

    /// Registers a callback for all response events by actions sended to AMI Asterisk.
    ///
    /// # Example
    ///
    /// ```
    /// let manager = ManagerBuilder::new(options)
    ///    .on_all_response_events(Box::new(|value| {
    ///       println!("Response event received: {}", value);
    ///   }))
    ///  .build();
    /// ```
    ///
    /// # Arguments
    ///
    /// * `callback` - A callback function that receives a `Value` object.
    pub fn on_all_response_events(mut self, callback: Box<dyn Fn(Value) + Send + Sync>) -> Self {
        self.response_event_callback = Some(callback);
        self
    }

    /// Consumes the builder and returns a `Manager` instance.
    pub fn build(self) -> Manager {
        Manager {
            options: self.options,
            connection: None,
            authenticated: false,
            emitter: Arc::new(Mutex::new(vec![])),
            event_sender: None,
            participants: HashMap::new(),
            event_callbacks: self.event_callbacks,
            global_callback: self.global_callback,
            raw_event_callback: self.raw_event_callback,
            response_event_callback: self.response_event_callback,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Participant {
    name: String,
    number: String,
    with: Option<String>,
}

impl Manager {
    /// Creates a new instance of the Manager.
    pub fn new(
        options: ManagerOptions,
        event_sender: Option<mpsc::Sender<String>>,
        _event_callback: Option<Box<dyn Fn(Value) + Send + Sync>>,
    ) -> Self {
        Manager {
            options,
            connection: None,
            authenticated: false,
            emitter: Arc::new(Mutex::new(vec![])),
            event_sender,
            participants: HashMap::new(),
            event_callbacks: HashMap::new(),
            global_callback: None,
            raw_event_callback: None,
            response_event_callback: None,
        }
    }

    /// Connects to the AMI server.
    pub async fn connect(&mut self) -> Result<(), String> {
        log::debug!("Connecting to {}:{}", self.options.host, self.options.port);
        let connection = timeout(
            Duration::from_secs(10),
            TcpStream::connect((self.options.host.as_str(), self.options.port)),
        )
        .await
        .map_err(|_| "Connection timed out".to_string())?
        .map_err(|e| e.to_string())?;

        self.connection = Some(connection);
        if let Some(sender) = &self.event_sender {
            sender.send("serverconnect".to_string()).await.unwrap();
        }
        self.authenticate().await
    }

    /// Connects to the AMI server with reconnection logic.
    pub async fn connect_with_retries(&mut self) {
        let mut attempts = 0;
        loop {
            match self.connect().await {
                Ok(_) => {
                    log::info!("Successfully connected to AMI server");
                    break;
                }
                Err(err) => {
                    log::error!("Failed to connect: {}", err);
                    attempts += 1;
                    let wait_time = if attempts > 10 {
                        Duration::from_secs(60)
                    } else {
                        Duration::from_secs(5)
                    };
                    log::info!("Reconnecting in {:?}", wait_time);
                    tokio::time::sleep(wait_time).await;
                }
            }
        }
    }

    /// Authenticates with the AMI server.
    async fn authenticate(&mut self) -> Result<(), String> {
        log::debug!("Authenticating with username: {}", self.options.username);
        let login_action = serde_json::json!({
            "Action": "Login",
            "event": if self.options.events { "on" } else { "off" },
            "secret": self.options.password,
            "username": self.options.username
        });

        self.send_action(login_action).await?;

        let response = self.read_data().await?;

        if response.contains("Response: Success") {
            self.authenticated = true;
            Ok(())
        } else {
            Err("Authentication failed".to_string())
        }
    }

    /// Sends an action to the AMI server.
    pub async fn send_action(&mut self, action: serde_json::Value) -> Result<(), String> {
        let mut action_str = String::new();
        for (key, value) in action.as_object().unwrap().iter() {
            action_str.push_str(&format!("{}: {}\r\n", key, value.as_str().unwrap_or("")));
        }
        action_str.push_str("\r\n");

        let connection = self
            .connection
            .as_mut()
            .ok_or("No connection established")?;
        connection
            .write_all(action_str.as_bytes())
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Reads data from the AMI server.
    pub async fn read_data(&mut self) -> Result<String, String> {
        let mut buffer = vec![0; 8192];
        let mut complete_data = String::new();
        let mut event_list_started = false;
        let mut is_response_event = false;

        let connection = self
            .connection
            .as_mut()
            .ok_or("No connection established")?;

        loop {
            let n = match connection.read(&mut buffer).await {
                Ok(n) if n == 0 => {
                    return Err("Connection lost".to_string());
                }
                Ok(n) => n,
                Err(e) => {
                    return Err(e.to_string());
                }
            };

            let data = String::from_utf8_lossy(&buffer[..n]);
            complete_data.push_str(&data);

            if data.contains("Response:") {
                is_response_event = true;
            }

            if data.contains("EventList: start") {
                event_list_started = true;
            }

            if event_list_started && data.contains("EventList: Complete") {
                break;
            }

            if !event_list_started && data.contains("\r\n\r\n") {
                break;
            }
        }

        if is_response_event {
            self.on_event(complete_data.clone(), "responseEvent").await;
        } else {
            for event in complete_data.split("\r\n\r\n") {
                if !event.trim().is_empty() {
                    self.on_event(event.to_string(), "rawEvent").await;
                }
            }
        }

        Ok(complete_data)
    }

    /// Reads data from the AMI server with reconnection logic.
    pub async fn read_data_with_retries(&mut self) {
        loop {
            match self.read_data().await {
                Ok(_) => {}
                Err(err) => {
                    log::error!("Error reading data: {}", err);
                    self.connect_with_retries().await;
                }
            }
        }
    }

    /// Processes an event received from the AMI server.
    pub async fn on_event(&mut self, event: String, event_type: &str) {
        let json_event = self.parse_event(&event).unwrap();
        let event_name = json_event["Event"].as_str().unwrap_or("");

        if let Some(callback) = self.event_callbacks.get(event_name) {
            callback(json_event.clone());
        }

        if let Some(global_callback) = &self.global_callback {
            global_callback(json_event.clone());
        }

        match event_type {
            "rawEvent" => {
                if let Some(raw_event_callback) = &self.raw_event_callback {
                    raw_event_callback(json_event.clone());
                }
            }
            "responseEvent" => {
                if let Some(response_event_callback) = &self.response_event_callback {
                    response_event_callback(json_event.clone());
                }
            }
            _ => {}
        }

        let get_case_insensitive = |obj: &serde_json::Value, key: &str| {
            obj.as_object()
                .and_then(|map| {
                    map.iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(key))
                        .map(|(_, v)| v.clone())
                })
                .unwrap_or_else(|| serde_json::Value::Null)
        };

        if let Some(sender) = &self.event_sender {
            match get_case_insensitive(&json_event, "Event")
                .as_str()
                .unwrap_or("")
            {
                "Newchannel" => {
                    let uniqueid = get_case_insensitive(&json_event, "UniqueID")
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let participant = Participant {
                        name: get_case_insensitive(&json_event, "calleridname")
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        number: get_case_insensitive(&json_event, "calleridnum")
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        with: None,
                    };
                    self.participants.insert(uniqueid, participant);
                }
                "Newcallerid" => {
                    let uniqueid = get_case_insensitive(&json_event, "UniqueID")
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    if let Some(participant) = self.participants.get_mut(&uniqueid) {
                        if participant.number.is_empty() {
                            participant.number = get_case_insensitive(&json_event, "callerid")
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                        }
                        if !get_case_insensitive(&json_event, "calleridname")
                            .as_str()
                            .unwrap_or("")
                            .starts_with('<')
                        {
                            participant.name = get_case_insensitive(&json_event, "calleridname")
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                        }
                    }
                }
                "Dial" => {
                    let src_uniqueid = get_case_insensitive(&json_event, "srcuniqueid")
                        .as_str()
                        .unwrap()
                        .to_string();
                    let dest_uniqueid = get_case_insensitive(&json_event, "destuniqueid")
                        .as_str()
                        .unwrap()
                        .to_string();
                    if let Some(src_participant) = self.participants.get_mut(&src_uniqueid) {
                        src_participant.with = Some(dest_uniqueid.clone());
                    }
                    if let Some(dest_participant) = self.participants.get_mut(&dest_uniqueid) {
                        dest_participant.with = Some(src_uniqueid.clone());
                    }
                    sender.send("dialing".to_string()).await.unwrap();
                }
                "Link" => {
                    let _uniqueid1 = get_case_insensitive(&json_event, "uniqueid1")
                        .as_str()
                        .unwrap()
                        .to_string();
                    let _uniqueid2 = get_case_insensitive(&json_event, "uniqueid2")
                        .as_str()
                        .unwrap()
                        .to_string();
                    sender.send("callconnected".to_string()).await.unwrap();
                }
                "Unlink" => {
                    let _uniqueid1 = get_case_insensitive(&json_event, "uniqueid1")
                        .as_str()
                        .unwrap()
                        .to_string();
                    let _uniqueid2 = get_case_insensitive(&json_event, "uniqueid2")
                        .as_str()
                        .unwrap()
                        .to_string();
                    sender.send("calldisconnected".to_string()).await.unwrap();
                }
                "Hold" => {
                    let _uniqueid = get_case_insensitive(&json_event, "uniqueid")
                        .as_str()
                        .unwrap()
                        .to_string();
                    sender.send("hold".to_string()).await.unwrap();
                }
                "Unhold" => {
                    let _uniqueid = get_case_insensitive(&json_event, "uniqueid")
                        .as_str()
                        .unwrap()
                        .to_string();
                    sender.send("unhold".to_string()).await.unwrap();
                }
                "Hangup" => {
                    let uniqueid = get_case_insensitive(&json_event, "uniqueid")
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    self.participants.remove(&uniqueid);
                    sender.send("hangup".to_string()).await.unwrap();
                }
                "Cdr" => {
                    let id_caller = get_case_insensitive(&json_event, "uniqueid")
                        .as_str()
                        .unwrap()
                        .to_string();
                    let id_callee = self
                        .participants
                        .get(&id_caller)
                        .and_then(|p| p.with.clone())
                        .unwrap_or_default();
                    let _status = get_case_insensitive(&json_event, "disposition")
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase();
                    sender.send("callreport".to_string()).await.unwrap();
                    self.participants.remove(&id_caller);
                    self.participants.remove(&id_callee);
                }
                _ => {}
            }
        }

        let mut emitter = self.emitter.lock().unwrap();
        emitter.push(event.clone());
    }

    /// Parses an event received from the AMI server.
    pub fn parse_event(&self, data: &str) -> Result<serde_json::Value, String> {
        let mut map = serde_json::Map::new();
        let mut events_map: std::collections::HashMap<String, Vec<serde_json::Value>> =
            std::collections::HashMap::new();
        let mut current_event = serde_json::Map::new();
        let mut in_event = false;
        let mut event_type = String::new();

        for line in data.lines() {
            if line.trim().is_empty() {
                if in_event {
                    events_map
                        .entry(event_type.clone())
                        .or_default()
                        .push(serde_json::Value::Object(current_event.clone()));
                    current_event.clear();
                    in_event = false;
                }
                continue;
            }

            if let Some((key, value)) = line.split_once(": ") {
                let key = key.trim().to_string();
                let value = value.trim().to_string();

                if key == "Event" {
                    if in_event {
                        events_map
                            .entry(event_type.clone())
                            .or_default()
                            .push(serde_json::Value::Object(current_event.clone()));
                        current_event.clear();
                    }
                    event_type = value.clone();
                    in_event = true;
                }

                if in_event {
                    current_event.insert(key, serde_json::Value::String(value));
                } else {
                    map.insert(key, serde_json::Value::String(value));
                }
            }
        }

        if in_event {
            events_map
                .entry(event_type.clone())
                .or_default()
                .push(serde_json::Value::Object(current_event));
        }

        for (event, events) in events_map {
            if events.len() == 1 {
                map.extend(events[0].as_object().unwrap().clone());
            } else {
                map.insert(event, serde_json::Value::Array(events));
            }
        }

        Ok(serde_json::Value::Object(map))
    }

    /// Disconnects from the AMI server.
    pub async fn disconnect(&mut self) {
        log::debug!("Disconnecting");
        if let Some(mut connection) = self.connection.take() {
            let _ = connection.shutdown().await;
        }
        if let Some(sender) = &self.event_sender {
            sender.send("serverdisconnect".to_string()).await.unwrap();
        }
    }

    /// Returns whether the manager is authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event_single() {
        let manager = Manager::new(
            ManagerOptions {
                port: 5038,
                host: "example.com".to_string(),
                username: "admin".to_string(),
                password: "password".to_string(),
                events: false,
            },
            None,
            None,
        );

        let data = "Event: TestEvent\r\nKey: Value\r\n\r\n";
        let parsed = manager.parse_event(data).unwrap();

        assert_eq!(parsed["Key"], "Value");
    }

    #[test]
    fn test_parse_event_multiple() {
        let manager = Manager::new(
            ManagerOptions {
                port: 5038,
                host: "example.com".to_string(),
                username: "admin".to_string(),
                password: "password".to_string(),
                events: false,
            },
            None,
            None,
        );

        let data =
            "Event: TestEvent\r\nKey1: Value1\r\n\r\nEvent: TestEvent\r\nKey2: Value2\r\n\r\n";
        let parsed = manager.parse_event(data).unwrap();

        assert_eq!(parsed["TestEvent"][0]["Key1"], "Value1");
        assert_eq!(parsed["TestEvent"][1]["Key2"], "Value2");
    }

    #[test]
    fn test_parse_event_no_event() {
        let manager = Manager::new(
            ManagerOptions {
                port: 5038,
                host: "example.com".to_string(),
                username: "admin".to_string(),
                password: "password".to_string(),
                events: false,
            },
            None,
            None,
        );

        let data = "Key: Value\r\n\r\n";
        let parsed = manager.parse_event(data).unwrap();

        assert_eq!(parsed["Key"], "Value");
    }
}
