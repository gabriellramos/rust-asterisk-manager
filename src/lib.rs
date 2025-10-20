//! # Asterisk Manager Library
//!
//! A modern, strongly-typed, stream-based library for integration with the Asterisk Manager Interface (AMI).
//!
//! - **Typed AMI messages**: Actions, Events, and Responses as Rust enums/structs.
//! - **Stream-based API**: Consume events via `tokio_stream`.
//! - **Asynchronous operations**: Fully based on Tokio.
//! - **Resilient connections**: Optional resilient module with heartbeat and automatic reconnection.
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use asterisk_manager::{Manager, ManagerOptions, AmiAction};
//! use tokio_stream::StreamExt;
//!
//! #[tokio::main]
//! async fn main() {
//!     let options = ManagerOptions {
//!         port: 5038,
//!         host: "127.0.0.1".to_string(),
//!         username: "admin".to_string(),
//!         password: "password".to_string(),
//!         events: true,
//!     };
//!     let mut manager = Manager::new();
//!     manager.connect_and_login(options).await.unwrap();
//!
//!     let mut events = manager.all_events_stream().await;
//!     tokio::spawn(async move {
//!         while let Some(Ok(ev)) = events.next().await {
//!             println!("Event: {:?}", ev);
//!         }
//!     });
//!
//!     let resp = manager.send_action(AmiAction::Ping { action_id: None }).await.unwrap();
//!     println!("Ping response: {:?}", resp);
//!     manager.disconnect().await.unwrap();
//! }
//! ```
//!
//! ## Resilient Connections
//!
//! For production applications that need automatic reconnection and heartbeat monitoring,
//! use the `resilient` module:
//!
//! ```rust,no_run
//! use asterisk_manager::resilient::{ResilientOptions, connect_resilient};
//! use asterisk_manager::ManagerOptions;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let options = ResilientOptions {
//!         manager_options: ManagerOptions {
//!             port: 5038,
//!             host: "127.0.0.1".to_string(),
//!             username: "admin".to_string(),
//!             password: "password".to_string(),
//!             events: true,
//!         },
//!         buffer_size: 2048,
//!         enable_heartbeat: true,
//!         enable_watchdog: true,
//!         heartbeat_interval: 30,
//!         watchdog_interval: 1,
//!         max_retries: 3,
//!         metrics: None,
//!         cumulative_attempts_counter: None,
//!     };
//!     
//!     let manager = connect_resilient(options).await?;
//!     // Manager now has heartbeat and automatic reconnection enabled
//!     Ok(())
//! }
//! ```
//!
//! ## Features
//!
//! - Login/logout, sending actions, and receiving AMI events.
//! - Support for common events (`Newchannel`, `Hangup`, `PeerStatus`) and fallback for unknown events.
//! - Detailed error handling via the `AmiError` enum.
//! - Configurable buffer sizes for high-throughput applications.
//! - Heartbeat monitoring with configurable interval and automatic disconnection on failure.
//! - Watchdog for automatic reconnection with configurable check interval when not authenticated.
//! - Infinite event streams that handle lag and reconnection automatically.
//!
//! ## Requirements
//!
//! - Rust 1.70+
//! - Tokio (asynchronous runtime)
//!
//! ## License
//!
//! MIT

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::time::{timeout, Duration};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::Stream;
use tokio_util::sync::CancellationToken;
#[cfg(feature = "docs")]
use utoipa::ToSchema;
use uuid::Uuid;

pub mod resilient;

#[cfg_attr(feature = "docs", derive(ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmiResponse {
    #[serde(rename = "Response")]
    pub response: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ActionID")]
    pub action_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "Message")]
    pub message: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "docs", schema(additional_properties = true))]
    pub fields: HashMap<String, Value>,
}

#[cfg_attr(feature = "docs", derive(ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "Action", rename_all = "PascalCase")]
pub enum AmiAction {
    Login {
        username: String,
        secret: String,
        #[serde(rename = "Events")]
        events: Option<String>,
        #[serde(rename = "ActionID")]
        action_id: Option<String>,
    },
    Logoff {
        #[serde(rename = "ActionID")]
        action_id: Option<String>,
    },
    Ping {
        #[serde(rename = "ActionID")]
        action_id: Option<String>,
    },
    Command {
        command: String,
        #[serde(rename = "ActionID")]
        action_id: Option<String>,
    },
    Custom {
        action: String,
        #[serde(flatten)]
        params: HashMap<String, String>,
        #[serde(rename = "ActionID")]
        action_id: Option<String>,
    },
}

#[cfg_attr(feature = "docs", derive(ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewchannelEventData {
    #[serde(rename = "Channel")]
    pub channel: String,
    #[serde(rename = "Uniqueid")]
    pub uniqueid: String,
    #[serde(rename = "ChannelState")]
    pub channel_state: Option<String>,
    #[serde(rename = "ChannelStateDesc")]
    pub channel_state_desc: Option<String>,
    #[serde(rename = "CallerIDNum")]
    pub caller_id_num: Option<String>,
    #[serde(rename = "CallerIDName")]
    pub caller_id_name: Option<String>,
    #[serde(flatten)]
    pub other: HashMap<String, String>,
}

#[cfg_attr(feature = "docs", derive(ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HangupEventData {
    #[serde(rename = "Channel")]
    pub channel: String,
    #[serde(rename = "Uniqueid")]
    pub uniqueid: String,
    #[serde(rename = "Cause")]
    pub cause: Option<String>,
    #[serde(rename = "Cause-txt")]
    pub cause_txt: Option<String>,
    #[serde(flatten)]
    pub other: HashMap<String, String>,
}

#[cfg_attr(feature = "docs", derive(ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatusEventData {
    #[serde(rename = "Peer")]
    pub peer: String,
    #[serde(rename = "PeerStatus")]
    pub peer_status: String,
    #[serde(flatten)]
    pub other: HashMap<String, String>,
}

#[cfg_attr(feature = "docs", derive(ToSchema))]
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum AmiEvent {
    Newchannel(NewchannelEventData),
    Hangup(HangupEventData),
    PeerStatus(PeerStatusEventData),
    UnknownEvent {
        event_type: String,
        fields: HashMap<String, String>,
    },
    InternalConnectionLost {
        error: String,
    },
}

impl<'de> Deserialize<'de> for AmiEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let map_obj = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("AmiEvent: Expected a JSON object/map"))?;

        if let Some(event_type_val) = map_obj.get("Event") {
            let event_type_str = event_type_val.as_str().ok_or_else(|| {
                serde::de::Error::custom("AmiEvent: 'Event' field is not a string")
            })?;

            match event_type_str {
                "Newchannel" => Ok(AmiEvent::Newchannel(
                    NewchannelEventData::deserialize(value.clone())
                        .map_err(serde::de::Error::custom)?,
                )),
                "Hangup" => Ok(AmiEvent::Hangup(
                    HangupEventData::deserialize(value.clone())
                        .map_err(serde::de::Error::custom)?,
                )),
                "PeerStatus" => Ok(AmiEvent::PeerStatus(
                    PeerStatusEventData::deserialize(value.clone())
                        .map_err(serde::de::Error::custom)?,
                )),
                _ => {
                    let fields: HashMap<String, String> = map_obj
                        .iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect();
                    Ok(AmiEvent::UnknownEvent {
                        event_type: event_type_str.to_string(),
                        fields,
                    })
                }
            }
        } else {
            let fields: HashMap<String, String> = map_obj
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect();
            Ok(AmiEvent::UnknownEvent {
                event_type: "UnknownOrMalformed".to_string(),
                fields,
            })
        }
    }
}

#[derive(Debug, Error)]
pub enum AmiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Serialize error: {0}")]
    SerializeError(String),
    #[error("JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("Action failed: {response:?}")]
    ActionFailed { response: AmiResponse },
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Operation timed out")]
    Timeout,
    #[error("Login required")]
    LoginRequired,
    #[error("Internal channel error: {0}")]
    ChannelError(String),
    #[error("Event stream lagged: {0}")]
    EventStreamLagged(#[from] tokio::sync::broadcast::error::RecvError),
    #[error("Not connected to AMI server")]
    NotConnected,
    #[error("Other error: {0}")]
    Other(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ManagerOptions {
    pub port: u16,
    pub host: String,
    pub username: String,
    pub password: String,
    pub events: bool,
}

struct InnerManager {
    authenticated: bool,
    /// Channel for sending raw AMI messages
    write_tx: Option<mpsc::Sender<String>>,
    /// Channel for receiving raw AMI messages
    event_broadcaster: broadcast::Sender<AmiEvent>,
    /// Responders mapped for each action ID
    pending_responses: HashMap<String, oneshot::Sender<Result<AmiResponse, AmiError>>>,
    /// Heartbeat cancellation token
    heartbeat_token: Option<CancellationToken>,
    /// Watchdog cancellation token
    watchdog_token: Option<CancellationToken>,
    /// Unique identifier for this manager instance (for logging)
    instance_id: String,
}

#[derive(Clone)]
pub struct Manager {
    pub(crate) inner: Arc<Mutex<InnerManager>>,
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

impl Manager {
    pub fn new() -> Self {
        Self::new_with_buffer(1024)
    }

    pub fn new_with_buffer(buffer_size: usize) -> Self {
        let (event_tx, _) = broadcast::channel(buffer_size);
        let instance_id = Uuid::new_v4().to_string()[..8].to_string();
        log::debug!("Creating new Manager instance [{instance_id}]");
        let inner = InnerManager {
            authenticated: false,
            write_tx: None,
            event_broadcaster: event_tx,
            pending_responses: HashMap::new(),
            heartbeat_token: None,
            watchdog_token: None,
            instance_id,
        };
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub async fn connect_and_login(&mut self, options: ManagerOptions) -> Result<(), AmiError> {
        let stream = timeout(
            Duration::from_secs(10),
            TcpStream::connect((options.host.as_str(), options.port)),
        )
        .await
        .map_err(|_| AmiError::Timeout)?
        .map_err(AmiError::Io)?;

        let (reader, writer) = stream.into_split();

        let (write_tx, write_rx) = mpsc::channel::<String>(100);
        let (dispatch_tx, dispatch_rx) = mpsc::channel::<String>(1024);

        let event_broadcaster = {
            let inner = self.inner.lock().await;
            inner.event_broadcaster.clone()
        };

        spawn_writer_task(writer, write_rx);
        spawn_reader_task(reader, dispatch_tx, event_broadcaster);
        spawn_dispatcher_task(self.inner.clone(), dispatch_rx);

        self.inner.lock().await.write_tx = Some(write_tx);

        let login_action = AmiAction::Login {
            username: options.username.clone(),
            secret: options.password.clone(),
            events: Some("on".to_string()),
            action_id: Some("rust-ami-login".to_string()),
        };

        match self.send_action(login_action).await {
            Ok(resp) if resp.response.eq_ignore_ascii_case("Success") => {
                self.inner.lock().await.authenticated = true;
                Ok(())
            }
            Ok(resp) => Err(AmiError::AuthenticationFailed(
                resp.message.unwrap_or_default(),
            )),
            Err(e) => Err(e),
        }
    }

    pub async fn send_action(&self, mut action: AmiAction) -> Result<AmiResponse, AmiError> {
        let action_id = get_or_set_action_id(&mut action);

        let mut stream = self.all_events_stream().await;

        let initial_response = self.send_initial_request(action.clone()).await?;

        if initial_response
            .fields
            .get("EventList")
            .and_then(|v| v.as_str())
            == Some("start")
        {
            let mut collected_events = Vec::new();

            let collection_result = tokio::time::timeout(Duration::from_secs(10), async {
                use tokio_stream::StreamExt;
                while let Some(Ok(event)) = stream.next().await {
                    if let AmiEvent::UnknownEvent { event_type, fields } = &event {
                        if fields.get("ActionID").map(|id| id.as_str()) == Some(&action_id) {
                            if event_type.ends_with("Complete") {
                                break;
                            }
                            collected_events.push(event.clone());
                        }
                    }
                }
            })
            .await;

            if collection_result.is_err() {
                return Err(AmiError::Timeout);
            }

            let mut final_fields = initial_response.fields;
            final_fields.insert(
                "CollectedEvents".to_string(),
                serde_json::to_value(&collected_events)?,
            );

            Ok(AmiResponse {
                response: initial_response.response,
                action_id: initial_response.action_id,
                message: Some("Successfully collected events.".to_string()),
                fields: final_fields,
            })
        } else {
            Ok(initial_response)
        }
    }

    async fn send_initial_request(&self, mut action: AmiAction) -> Result<AmiResponse, AmiError> {
        let action_id = get_or_set_action_id(&mut action);
        let (tx, rx) = oneshot::channel();
        let action_str = serialize_ami_action(&action)?;

        {
            let mut inner = self.inner.lock().await;
            if inner.write_tx.is_none() {
                return Err(AmiError::NotConnected);
            }
            inner.pending_responses.insert(action_id.clone(), tx);
            let writer = inner.write_tx.as_ref().unwrap();
            if writer.send(action_str).await.is_err() {
                inner.pending_responses.remove(&action_id);
                return Err(AmiError::ConnectionClosed);
            }
        }

        match timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(Ok(resp))) => Ok(resp),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(AmiError::ChannelError("Responder dropped".to_string())),
            Err(_) => Err(AmiError::Timeout),
        }
    }

    pub async fn disconnect(&self) -> Result<(), AmiError> {
        let mut inner = self.inner.lock().await;
        inner.write_tx = None;
        inner.authenticated = false;

        // Cancel heartbeat and watchdog
        if let Some(token) = &inner.heartbeat_token {
            token.cancel();
            inner.heartbeat_token = None;
        }
        if let Some(token) = &inner.watchdog_token {
            token.cancel();
            inner.watchdog_token = None;
        }

        Ok(())
    }

    pub async fn is_authenticated(&self) -> bool {
        self.inner.lock().await.authenticated
    }

    pub async fn all_events_stream(
        &self,
    ) -> impl Stream<Item = Result<AmiEvent, BroadcastStreamRecvError>> + Send + Unpin {
        let inner = self.inner.lock().await;
        BroadcastStream::new(inner.event_broadcaster.subscribe())
    }

    /// Start heartbeat with default interval (30 seconds). Kept for backwards compatibility.
    pub async fn start_heartbeat(&self) -> Result<(), AmiError> {
        self.start_heartbeat_with_interval(30).await
    }

    /// Start the heartbeat task with a configurable interval (in seconds).
    pub async fn start_heartbeat_with_interval(&self, interval_secs: u64) -> Result<(), AmiError> {
        let mut inner = self.inner.lock().await;
        let instance_id = inner.instance_id.clone();

        // Cancel existing heartbeat if any
        if let Some(token) = &inner.heartbeat_token {
            log::debug!("[{instance_id}] Cancelling existing heartbeat task");
            token.cancel();
        }

        let token = CancellationToken::new();
        inner.heartbeat_token = Some(token.clone());

        log::debug!("[{instance_id}] Starting heartbeat task (interval={interval_secs}s)");

        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            log::debug!("[{instance_id}] Heartbeat task started");
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        log::debug!("[{instance_id}] Heartbeat task cancelled");
                        break;
                    }
                    _ = interval.tick() => {
                        if manager.is_authenticated().await {
                            match manager.send_action(AmiAction::Ping { action_id: None }).await {
                                Ok(_) => {
                                    log::debug!("[{instance_id}] Heartbeat ping successful");
                                }
                                Err(e) => {
                                    log::warn!("[{instance_id}] Heartbeat ping failed: {e}");
                                    // Emit connection lost event
                                    if let Ok(inner) = manager.inner.try_lock() {
                                        let _ = inner.event_broadcaster.send(AmiEvent::InternalConnectionLost {
                                            error: format!("Heartbeat failed: {e}"),
                                        });
                                    }
                                    // Disconnect on heartbeat failure
                                    let _ = manager.disconnect().await;
                                    break;
                                }
                            }
                        } else {
                            log::trace!("[{instance_id}] Heartbeat tick: not authenticated, skipping ping");
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn start_watchdog(&self, options: ManagerOptions) -> Result<(), AmiError> {
        let instance_id = self.inner.lock().await.instance_id.clone();
        log::debug!(
            "[{}] Starting watchdog (default interval=1s) for user '{}' at {}:{}",
            instance_id,
            options.username,
            options.host,
            options.port
        );
        self.start_watchdog_with_interval(options, 1).await
    }

    pub async fn start_watchdog_with_interval(
        &self,
        options: ManagerOptions,
        interval_secs: u64,
    ) -> Result<(), AmiError> {
        let mut inner = self.inner.lock().await;
        let instance_id = inner.instance_id.clone();

        // Cancel existing watchdog if any
        if let Some(token) = &inner.watchdog_token {
            log::debug!(
                "[{instance_id}] Cancelling existing watchdog task before starting a new one"
            );
            token.cancel();
        }

        let token = CancellationToken::new();
        inner.watchdog_token = Some(token.clone());

        log::debug!(
            "[{}] Spawning watchdog task (interval={}s) for user '{}' at {}:{}",
            instance_id,
            interval_secs,
            options.username,
            options.host,
            options.port
        );

        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            log::debug!(
                "[{}] Watchdog task started (interval={}s) for '{}'@{}:{}",
                instance_id,
                interval_secs,
                options.username,
                options.host,
                options.port
            );
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        log::debug!("[{instance_id}] Watchdog task cancelled by token");
                        break;
                    }
                    _ = interval.tick() => {
                        if !manager.is_authenticated().await {
                            log::debug!(
                                "[{}] Watchdog attempting reconnection to '{}'@{}:{}...",
                                instance_id,
                                options.username, options.host, options.port
                            );
                            let mut mgr = manager.clone();
                            match mgr.connect_and_login(options.clone()).await {
                                Ok(_) => {
                                    log::info!(
                                        "[{}] Watchdog reconnection successful to '{}'@{}:{}",
                                        instance_id,
                                        options.username, options.host, options.port
                                    );
                                }
                                Err(e) => {
                                    log::debug!(
                                        "[{}] Watchdog reconnection to '{}'@{}:{} failed: {}",
                                        instance_id,
                                        options.username, options.host, options.port, e
                                    );
                                }
                            }
                        } else {
                            log::trace!("[{instance_id}] Watchdog tick: already authenticated; no action taken");
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

fn spawn_writer_task(mut writer: OwnedWriteHalf, mut write_rx: mpsc::Receiver<String>) {
    tokio::spawn(async move {
        while let Some(action_str) = write_rx.recv().await {
            if writer.write_all(action_str.as_bytes()).await.is_err() {
                break;
            }
        }
    });
}

fn spawn_reader_task(
    reader: OwnedReadHalf,
    dispatch_tx: mpsc::Sender<String>,
    event_broadcaster: broadcast::Sender<AmiEvent>,
) {
    tokio::spawn(async move {
        let mut buf_reader = BufReader::new(reader);
        loop {
            let mut message_block = String::new();
            loop {
                let mut line = String::new();
                match buf_reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => {
                        // Connection lost - emit synthetic event
                        let _ = event_broadcaster.send(AmiEvent::InternalConnectionLost {
                            error: "Connection lost during read".to_string(),
                        });
                        return;
                    }
                    Ok(_) => {
                        let is_end = line == "\r\n";
                        message_block.push_str(&line);
                        if is_end {
                            break;
                        }
                    }
                }
            }

            if !message_block.trim().is_empty() && dispatch_tx.send(message_block).await.is_err() {
                let _ = event_broadcaster.send(AmiEvent::InternalConnectionLost {
                    error: "Dispatcher channel closed".to_string(),
                });
                break;
            }
        }
    });
}

fn spawn_dispatcher_task(
    inner_arc: Arc<Mutex<InnerManager>>,
    mut dispatch_rx: mpsc::Receiver<String>,
) {
    tokio::spawn(async move {
        while let Some(raw_message) = dispatch_rx.recv().await {
            if let Ok(parsed_messages) = parse_ami_protocol_message(&raw_message) {
                for value_msg in parsed_messages {
                    let mut inner = inner_arc.lock().await;
                    if value_msg.get("Response").is_some() {
                        if let Ok(resp) = serde_json::from_value::<AmiResponse>(value_msg) {
                            if let Some(action_id) = &resp.action_id {
                                if let Some(responder) = inner.pending_responses.remove(action_id) {
                                    let _ = responder.send(Ok(resp));
                                }
                            }
                        }
                    } else if value_msg.get("Event").is_some() {
                        if let Ok(event) = serde_json::from_value::<AmiEvent>(value_msg.clone()) {
                            let _ = inner.event_broadcaster.send(event);
                        }
                    }
                }
            }
        }
    });
}

fn parse_ami_protocol_message(raw_data: &str) -> Result<Vec<serde_json::Value>, AmiError> {
    let mut messages = Vec::new();
    for block in raw_data.trim().split("\r\n\r\n") {
        if block.is_empty() {
            continue;
        }
        let mut map = serde_json::Map::new();
        for line in block.lines() {
            if let Some((key, value)) = line.split_once(": ") {
                map.insert(
                    key.trim().to_string(),
                    serde_json::Value::String(value.trim().to_string()),
                );
            }
        }
        if !map.is_empty() {
            messages.push(serde_json::Value::Object(map));
        }
    }
    Ok(messages)
}

fn serialize_ami_action(action: &AmiAction) -> Result<String, AmiError> {
    let mut s = String::new();
    match action {
        AmiAction::Login {
            username,
            secret,
            events,
            action_id,
        } => {
            s.push_str("Action: Login\r\n");
            s.push_str(&format!("Username: {username}\r\n"));
            s.push_str(&format!("Secret: {secret}\r\n"));
            if let Some(ev) = events {
                s.push_str(&format!("Events: {ev}\r\n"));
            }
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {id}\r\n"));
            }
        }
        AmiAction::Logoff { action_id } => {
            s.push_str("Action: Logoff\r\n");
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {id}\r\n"));
            }
        }
        AmiAction::Ping { action_id } => {
            s.push_str("Action: Ping\r\n");
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {id}\r\n"));
            }
        }
        AmiAction::Command { command, action_id } => {
            s.push_str("Action: Command\r\n");
            s.push_str(&format!("Command: {command}\r\n"));
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {id}\r\n"));
            }
        }
        AmiAction::Custom {
            action: action_name,
            params,
            action_id,
        } => {
            s.push_str(&format!("Action: {action_name}\r\n"));
            for (k, v) in params {
                s.push_str(&format!("{k}: {v}\r\n"));
            }
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {id}\r\n"));
            }
        }
    }
    s.push_str("\r\n");
    Ok(s)
}

fn get_or_set_action_id(action: &mut AmiAction) -> String {
    match action {
        AmiAction::Login { action_id, .. }
        | AmiAction::Logoff { action_id }
        | AmiAction::Ping { action_id }
        | AmiAction::Command { action_id, .. }
        | AmiAction::Custom { action_id, .. } => {
            if let Some(id) = action_id {
                id.clone()
            } else {
                let new_id = Uuid::new_v4().to_string();
                *action_id = Some(new_id.clone());
                new_id
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[test]
    fn test_serialize_login_action() {
        let action = AmiAction::Login {
            username: "user".to_string(),
            secret: "pass".to_string(),
            events: Some("on".to_string()),
            action_id: Some("abc123".to_string()),
        };
        let s = serialize_ami_action(&action).unwrap();
        assert!(s.contains("Action: Login"));
        assert!(s.contains("Username: user"));
        assert!(s.contains("Secret: pass"));
        assert!(s.contains("Events: on"));
        assert!(s.contains("ActionID: abc123"));
        assert!(s.ends_with("\r\n\r\n"));
    }

    #[test]
    fn test_serialize_command_action() {
        let action = AmiAction::Command {
            command: "sip show peers".to_string(),
            action_id: None,
        };
        let s = serialize_ami_action(&action).unwrap();
        assert!(s.contains("Action: Command"));
        assert!(s.contains("Command: sip show peers"));
    }

    #[test]
    fn test_parse_ami_protocol_message() {
        let raw = "Response: Success\r\nActionID: 123\r\nMessage: Authentication accepted\r\n\r\n";
        let parsed = parse_ami_protocol_message(raw).unwrap();
        assert_eq!(parsed.len(), 1);
        let obj = &parsed[0];
        assert_eq!(obj["Response"], "Success");
        assert_eq!(obj["ActionID"], "123");
        assert_eq!(obj["Message"], "Authentication accepted");
    }

    #[test]
    fn test_deserialize_ami_response() {
        let raw = "Response: Success\r\nActionID: 123\r\nMessage: Authentication accepted\r\n\r\n";
        let parsed = parse_ami_protocol_message(raw).unwrap();
        let resp: AmiResponse = serde_json::from_value(parsed[0].clone()).unwrap();
        assert_eq!(resp.response, "Success");
        assert_eq!(resp.action_id.as_deref(), Some("123"));
        assert_eq!(resp.message.as_deref(), Some("Authentication accepted"));
    }

    #[test]
    fn test_deserialize_newchannel_event() {
        let raw = "Event: Newchannel\r\nChannel: SIP/100-00000001\r\nUniqueid: 1234\r\nChannelState: 4\r\nChannelStateDesc: Ring\r\nCallerIDNum: 100\r\nCallerIDName: Alice\r\n\r\n";
        let parsed = parse_ami_protocol_message(raw).unwrap();
        let event: AmiEvent = serde_json::from_value(parsed[0].clone()).unwrap();
        match event {
            AmiEvent::Newchannel(data) => {
                assert_eq!(data.channel, "SIP/100-00000001");
                assert_eq!(data.uniqueid, "1234");
                assert_eq!(data.channel_state.as_deref(), Some("4"));
                assert_eq!(data.channel_state_desc.as_deref(), Some("Ring"));
                assert_eq!(data.caller_id_num.as_deref(), Some("100"));
                assert_eq!(data.caller_id_name.as_deref(), Some("Alice"));
            }
            _ => panic!("Expected AmiEvent::Newchannel"),
        }
    }

    #[test]
    fn test_deserialize_hangup_event() {
        let raw = "Event: Hangup\r\nChannel: SIP/100-00000001\r\nUniqueid: 1234\r\nCause: 16\r\nCause-txt: Normal Clearing\r\n\r\n";
        let parsed = parse_ami_protocol_message(raw).unwrap();
        let event: AmiEvent = serde_json::from_value(parsed[0].clone()).unwrap();
        match event {
            AmiEvent::Hangup(data) => {
                assert_eq!(data.channel, "SIP/100-00000001");
                assert_eq!(data.uniqueid, "1234");
                assert_eq!(data.cause.as_deref(), Some("16"));
                assert_eq!(data.cause_txt.as_deref(), Some("Normal Clearing"));
            }
            _ => panic!("Expected AmiEvent::Hangup"),
        }
    }

    #[test]
    fn test_deserialize_peerstatus_event() {
        let raw = "Event: PeerStatus\r\nPeer: SIP/100\r\nPeerStatus: Registered\r\n\r\n";
        let parsed = parse_ami_protocol_message(raw).unwrap();
        let event: AmiEvent = serde_json::from_value(parsed[0].clone()).unwrap();
        match event {
            AmiEvent::PeerStatus(data) => {
                assert_eq!(data.peer, "SIP/100");
                assert_eq!(data.peer_status, "Registered");
            }
            _ => panic!("Expected AmiEvent::PeerStatus"),
        }
    }

    #[test]
    fn test_deserialize_unknown_event() {
        let raw = "Event: FooBar\r\nSomeField: Value\r\n\r\n";
        let parsed = parse_ami_protocol_message(raw).unwrap();
        let event: AmiEvent = serde_json::from_value(parsed[0].clone()).unwrap();
        match event {
            AmiEvent::UnknownEvent { event_type, fields } => {
                assert_eq!(event_type, "FooBar");
                assert_eq!(fields.get("SomeField").map(|s| s.as_str()), Some("Value"));
            }
            _ => panic!("Expected AmiEvent::UnknownEvent"),
        }
    }

    #[tokio::test]
    async fn test_manager_options_clone() {
        let opts = ManagerOptions {
            port: 5038,
            host: "localhost".to_string(),
            username: "admin".to_string(),
            password: "pwd".to_string(),
            events: true,
        };
        let opts2 = opts.clone();
        assert_eq!(opts.port, opts2.port);
        assert_eq!(opts.host, opts2.host);
        assert_eq!(opts.username, opts2.username);
        assert_eq!(opts.password, opts2.password);
        assert_eq!(opts.events, opts2.events);
    }

    #[tokio::test]
    async fn test_manager_new_and_auth_flag() {
        // A criação de `opts` não é mais necessária para este teste.
        let manager = Manager::new(); // Manager::new() agora não tem argumentos.
        assert!(!manager.is_authenticated().await);
    }

    #[tokio::test]
    async fn test_event_internal_connection_lost() {
        // 1. Cria um manager vazio, como no teste anterior.
        let manager = Manager::new();

        // 2. Get the event stream BEFORE sending the event.
        let mut stream = manager.all_events_stream().await;

        // 3. Send the event internally to simulate a disconnection.
        //    This part works again because of `pub(crate)`.
        {
            let inner = manager.inner.lock().await;
            let _ = inner
                .event_broadcaster
                .send(AmiEvent::InternalConnectionLost {
                    error: "simulated".to_string(),
                });
        }

        // 4. Verifica se o evento foi recebido corretamente pelo stream.
        let ev = stream.next().await.unwrap().unwrap();
        match ev {
            AmiEvent::InternalConnectionLost { error } => {
                assert_eq!(error, "simulated");
            }
            _ => panic!("Expected InternalConnectionLost"),
        }
    }

    #[tokio::test]
    async fn test_manager_options_default() {
        let opts = ManagerOptions {
            port: 5038,
            host: "localhost".to_string(),
            username: "admin".to_string(),
            password: "pwd".to_string(),
            events: true,
        };
        assert!(opts.events);
    }

    #[tokio::test]
    async fn test_manager_new_with_buffer() {
        let manager = Manager::new_with_buffer(512);
        assert!(!manager.is_authenticated().await);

        // Test that the buffer size is respected by checking we can create the stream
        let _stream = manager.all_events_stream().await;
    }

    #[tokio::test]
    async fn test_heartbeat_and_watchdog_tokens() {
        let manager = Manager::new();

        // Initially no tokens should be set
        {
            let inner = manager.inner.lock().await;
            assert!(inner.heartbeat_token.is_none());
            assert!(inner.watchdog_token.is_none());
        }

        // Create dummy options for testing
        let opts = ManagerOptions {
            port: 5038,
            host: "127.0.0.1".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            events: true,
        };

        // Start heartbeat should set token (even though connection will fail)
        let _ = manager.start_heartbeat().await;
        {
            let inner = manager.inner.lock().await;
            assert!(inner.heartbeat_token.is_some());
        }

        // Start watchdog should set token
        let _ = manager.start_watchdog(opts).await;
        {
            let inner = manager.inner.lock().await;
            assert!(inner.watchdog_token.is_some());
        }

        // Disconnect should clear both tokens
        let _ = manager.disconnect().await;
        {
            let inner = manager.inner.lock().await;
            assert!(inner.heartbeat_token.is_none());
            assert!(inner.watchdog_token.is_none());
        }
    }

    #[tokio::test]
    async fn test_connection_lost_event_emission() {
        // Test that synthetic connection lost events are properly emitted
        let manager = Manager::new();
        let mut stream = manager.all_events_stream().await;

        // Manually emit a connection lost event
        {
            let inner = manager.inner.lock().await;
            let _ = inner
                .event_broadcaster
                .send(AmiEvent::InternalConnectionLost {
                    error: "test connection lost".to_string(),
                });
        }

        // Verify the event is received
        let event = stream.next().await.unwrap().unwrap();
        match event {
            AmiEvent::InternalConnectionLost { error } => {
                assert_eq!(error, "test connection lost");
            }
            _ => panic!("Expected InternalConnectionLost event"),
        }
    }

    #[tokio::test]
    async fn test_heartbeat_interval_respected() {
        // Use tokio time control to test heartbeat scheduling
        tokio::time::pause();

        let manager = Manager::new();

        // Start heartbeat with a short interval
        let _ = manager.start_heartbeat_with_interval(2).await;

        // Advance time less than interval: no ticks yet
        tokio::time::advance(Duration::from_secs(1)).await;
        {
            let inner = manager.inner.lock().await;
            // Token should be set
            assert!(inner.heartbeat_token.is_some());
        }

        // Advance time to trigger at least one tick
        tokio::time::advance(Duration::from_secs(2)).await;

        // There's no direct public hook for verifying pings were sent without mocking
        // but we can assert that the heartbeat task remains active and didn't panic.
        // Ensure token still exists
        {
            let inner = manager.inner.lock().await;
            assert!(inner.heartbeat_token.is_some());
        }

        // Clean up
        let _ = manager.disconnect().await;
    }

    #[tokio::test]
    async fn test_watchdog_interval_configuration() {
        // Test that watchdog can be started with different intervals
        let manager = Manager::new();

        let opts = ManagerOptions {
            port: 5038,
            host: "127.0.0.1".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            events: true,
        };

        // Test default interval (backward compatibility)
        let _ = manager.start_watchdog(opts.clone()).await;
        {
            let inner = manager.inner.lock().await;
            assert!(inner.watchdog_token.is_some());
        }

        // Test custom interval
        let _ = manager.start_watchdog_with_interval(opts.clone(), 5).await;
        {
            let inner = manager.inner.lock().await;
            assert!(inner.watchdog_token.is_some());
        }

        // Clean up
        let _ = manager.disconnect().await;
    }
}
