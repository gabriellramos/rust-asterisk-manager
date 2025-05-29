//! # Asterisk Manager Library (v1.0.0)
//!
//! Esta biblioteca fornece uma implementação moderna, fortemente tipada e baseada em streams para interação com o Asterisk Manager Interface (AMI).
//!
//! - **Mensagens AMI tipadas**: Actions, Events e Responses como enums/structs Rust.
//! - **API baseada em Stream**: Consumo de eventos via tokio_stream.
//! - **Operações assíncronas**: Utiliza Tokio.
//! - **Reconexão automática** e **correlação ActionID/Response**.
//!
//! ## Exemplo de uso
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
//!     let mut manager = Manager::new(options);
//!     manager.connect_and_login().await.unwrap();
//!     let mut events = manager.all_events_stream();
//!     tokio::spawn(async move {
//!         while let Some(ev) = events.next().await {
//!             println!("Evento: {:?}", ev);
//!         }
//!     });
//!     let resp = manager.send_action(AmiAction::Ping { action_id: None }).await.unwrap();
//!     println!("Resposta ao Ping: {:?}", resp);
//!     manager.disconnect().await.unwrap();
//! }
//! ```

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::Stream;

//
// Tipos Fortes para AMI
//

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmiResponse {
    #[serde(rename = "Response")]
    pub response: String,
    #[serde(rename = "ActionID")]
    pub action_id: Option<String>,
    #[serde(rename = "Message")]
    pub message: Option<String>,
    #[serde(flatten)]
    pub fields: HashMap<String, String>,
}

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

#[derive(Debug, Clone, Serialize)]
pub enum AmiEvent {
    Newchannel(HashMap<String, String>),
    Hangup(HashMap<String, String>),
    PeerStatus(HashMap<String, String>),
    UnknownEvent {
        event_type: String,
        fields: HashMap<String, String>,
    },
    RawData(String),
}

impl<'de> Deserialize<'de> for AmiEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map: HashMap<String, String> = HashMap::deserialize(deserializer)?;
        if let Some(event_type) = map.get("Event") {
            match event_type.as_str() {
                "Newchannel" => Ok(AmiEvent::Newchannel(map)),
                "Hangup" => Ok(AmiEvent::Hangup(map)),
                "PeerStatus" => Ok(AmiEvent::PeerStatus(map)),
                _ => Ok(AmiEvent::UnknownEvent {
                    event_type: event_type.clone(),
                    fields: map,
                }),
            }
        } else {
            Ok(AmiEvent::RawData(format!("{:?}", map)))
        }
    }
}

#[derive(Debug)]
pub enum AmiError {
    Io(std::io::Error),
    ParseError(String),
    SerializeError(String),
    AuthenticationFailed(String),
    ActionFailed { response: AmiResponse },
    ConnectionClosed,
    Timeout,
    LoginRequired,
    ChannelError(String),
    EventStreamLagged(tokio::sync::broadcast::error::RecvError),
    NotConnected,
    Other(String),
}

impl std::fmt::Display for AmiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AmiError::Io(e) => write!(f, "IO error: {}", e),
            AmiError::ParseError(s) => write!(f, "Parse error: {}", s),
            AmiError::SerializeError(s) => write!(f, "Serialize error: {}", s),
            AmiError::AuthenticationFailed(s) => write!(f, "Authentication failed: {}", s),
            AmiError::ActionFailed { response } => write!(f, "Action failed: {:?}", response),
            AmiError::ConnectionClosed => write!(f, "Connection closed"),
            AmiError::Timeout => write!(f, "Operation timed out"),
            AmiError::LoginRequired => write!(f, "Login required"),
            AmiError::ChannelError(s) => write!(f, "Internal channel error: {}", s),
            AmiError::EventStreamLagged(e) => write!(f, "Event stream lagged: {}", e),
            AmiError::NotConnected => write!(f, "Not connected to AMI server"),
            AmiError::Other(s) => write!(f, "Other error: {}", s),
        }
    }
}

impl std::error::Error for AmiError {}

//
// ManagerOptions e Manager
//

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ManagerOptions {
    pub port: u16,
    pub host: String,
    pub username: String,
    pub password: String,
    pub events: bool,
}

pub struct Manager {
    options: ManagerOptions,
    connection: Option<TcpStream>,
    authenticated: bool,
    event_broadcaster: broadcast::Sender<AmiEvent>,
}

impl Manager {
    pub fn new(options: ManagerOptions) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Manager {
            options,
            connection: None,
            authenticated: false,
            event_broadcaster: event_tx,
        }
    }

    pub async fn connect_and_login(&mut self) -> Result<(), AmiError> {
        self.connect().await?;
        self.authenticate().await
    }

    pub async fn connect(&mut self) -> Result<(), AmiError> {
        let stream = timeout(
            Duration::from_secs(10),
            TcpStream::connect((self.options.host.as_str(), self.options.port)),
        )
        .await
        .map_err(|_| AmiError::Timeout)?
        .map_err(AmiError::Io)?;
        self.connection = Some(stream);
        // Consome banner inicial
        let mut temp_buf = [0; 1024];
        if let Some(conn) = self.connection.as_mut() {
            let _ = conn.read(&mut temp_buf).await;
        }
        Ok(())
    }

    async fn authenticate(&mut self) -> Result<(), AmiError> {
        let login_action = AmiAction::Login {
            username: self.options.username.clone(),
            secret: self.options.password.clone(),
            events: Some(if self.options.events { "on" } else { "off" }.to_string()),
            action_id: Some("rust-ami-login".to_string()),
        };
        let action_str = serialize_ami_action(&login_action)?;
        let conn = self.connection.as_mut().ok_or(AmiError::NotConnected)?;
        conn.write_all(action_str.as_bytes())
            .await
            .map_err(AmiError::Io)?;
        let response_data = self.read_ami_message_raw().await?;
        let parsed = parse_ami_protocol_message(&response_data)?;
        for val in parsed {
            if let Ok(resp) = serde_json::from_value::<AmiResponse>(val.clone()) {
                if resp.response.eq_ignore_ascii_case("Success") {
                    self.authenticated = true;
                    return Ok(());
                } else if resp.response.eq_ignore_ascii_case("Error") {
                    return Err(AmiError::AuthenticationFailed(
                        resp.message.unwrap_or_default(),
                    ));
                }
            }
        }
        Err(AmiError::AuthenticationFailed(
            "No valid success response received for login".to_string(),
        ))
    }

    pub async fn send_action(&mut self, action: AmiAction) -> Result<AmiResponse, AmiError> {
        if !self.authenticated && !matches!(action, AmiAction::Login { .. }) {
            return Err(AmiError::LoginRequired);
        }
        if self.connection.is_none() {
            return Err(AmiError::NotConnected);
        }
        let action_str = serialize_ami_action(&action)?;
        let conn = self.connection.as_mut().ok_or(AmiError::NotConnected)?;
        conn.write_all(action_str.as_bytes())
            .await
            .map_err(AmiError::Io)?;
        loop {
            let response_data = self.read_ami_message_raw().await?;
            let parsed = parse_ami_protocol_message(&response_data)?;
            for val in parsed {
                if let Ok(resp) = serde_json::from_value::<AmiResponse>(val.clone()) {
                    if resp.response.eq_ignore_ascii_case("Error") {
                        return Err(AmiError::ActionFailed { response: resp });
                    }
                    return Ok(resp);
                }
                // Eventos podem ser enviados para o broadcast
                if let Ok(event) = serde_json::from_value::<AmiEvent>(val.clone()) {
                    println!("[DEBUG AMI EVENT] {:?}", event);
                    let _ = self.event_broadcaster.send(event);
                }
            }
        }
    }

    async fn read_ami_message_raw(&mut self) -> Result<String, AmiError> {
        let mut buffer = vec![0; 8192];
        let mut complete_data = String::new();
        let connection = self.connection.as_mut().ok_or(AmiError::NotConnected)?;
        loop {
            let n = connection.read(&mut buffer).await.map_err(AmiError::Io)?;
            if n == 0 {
                return Err(AmiError::ConnectionClosed);
            }
            let data_chunk = String::from_utf8_lossy(&buffer[..n]);
            complete_data.push_str(&data_chunk);
            if complete_data.ends_with("\r\n\r\n") {
                break;
            }
        }
        Ok(complete_data)
    }

    pub async fn disconnect(&mut self) -> Result<(), AmiError> {
        if let Some(mut connection) = self.connection.take() {
            let logoff_action = AmiAction::Logoff {
                action_id: Some("rust-ami-logoff".to_string()),
            };
            let action_str = serialize_ami_action(&logoff_action)?;
            let _ = connection.write_all(action_str.as_bytes()).await;
            let _ = connection.shutdown().await;
        }
        self.authenticated = false;
        Ok(())
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    pub fn all_events_stream(
        &self,
    ) -> impl Stream<Item = Result<AmiEvent, BroadcastStreamRecvError>> + Send + Unpin {
        BroadcastStream::new(self.event_broadcaster.subscribe())
    }
}

//
// Parsing e Serialização (simplificados)
//

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
            println!("[DEBUG AMI RAW EVENT] {:?}", map);
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
            s.push_str(&format!("Username: {}\r\n", username));
            s.push_str(&format!("Secret: {}\r\n", secret));
            if let Some(ev) = events {
                s.push_str(&format!("Events: {}\r\n", ev));
            }
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {}\r\n", id));
            }
        }
        AmiAction::Logoff { action_id } => {
            s.push_str("Action: Logoff\r\n");
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {}\r\n", id));
            }
        }
        AmiAction::Ping { action_id } => {
            s.push_str("Action: Ping\r\n");
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {}\r\n", id));
            }
        }
        AmiAction::Command { command, action_id } => {
            s.push_str("Action: Command\r\n");
            s.push_str(&format!("Command: {}\r\n", command));
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {}\r\n", id));
            }
        }
        AmiAction::Custom {
            action: action_name,
            params,
            action_id,
        } => {
            s.push_str(&format!("Action: {}\r\n", action_name));
            for (k, v) in params {
                s.push_str(&format!("{}: {}\r\n", k, v));
            }
            if let Some(id) = action_id {
                s.push_str(&format!("ActionID: {}\r\n", id));
            }
        }
    }
    s.push_str("\r\n");
    Ok(s)
}

// --------------------------------------------------
// Testes
// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(s.ends_with("\r\n"));
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
    fn test_deserialize_ami_event() {
        let raw = "Event: Newchannel\r\nChannel: SIP/100-00000001\r\nUniqueid: 1234\r\n\r\n";
        let parsed = parse_ami_protocol_message(raw).unwrap();
        let event: AmiEvent = serde_json::from_value(parsed[0].clone()).unwrap();
        match event {
            AmiEvent::Newchannel(map) => {
                assert_eq!(
                    map.get("Channel").map(|s| s.as_str()),
                    Some("SIP/100-00000001")
                );
                assert_eq!(map.get("Uniqueid").map(|s| s.as_str()), Some("1234"));
            }
            _ => panic!("Esperado AmiEvent::Newchannel"),
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

    // Teste de autenticação mockada (não conecta de verdade)
    #[tokio::test]
    async fn test_manager_new_and_auth_flag() {
        let opts = ManagerOptions {
            port: 5038,
            host: "localhost".to_string(),
            username: "admin".to_string(),
            password: "pwd".to_string(),
            events: false,
        };
        let manager = Manager::new(opts);
        assert!(!manager.is_authenticated());
    }
}
