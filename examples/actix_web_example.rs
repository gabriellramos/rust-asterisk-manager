use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use asterisk_manager::{AmiAction, AmiError, AmiEvent, AmiResponse, Manager, ManagerOptions};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio_stream::StreamExt;

const RETRY_LIMIT: usize = 100;
const MAX_EVENT_BUFFER_SIZE: usize = 1000;

#[derive(Clone)]
struct AppState {
    manager: Arc<Mutex<Manager>>,
    events: Arc<Mutex<Vec<AmiEvent>>>,
    manager_options: ManagerOptions,
}

#[derive(Deserialize)]
struct ActionRequest {
    action: String,
    params: Option<std::collections::HashMap<String, String>>,
}

#[derive(Serialize)]
struct ActionResponse {
    result: String,
    response: Option<AmiResponse>,
    error: Option<String>,
}

#[derive(Serialize)]
struct CallsResponse {
    calls: Vec<serde_json::Value>,
}

async fn get_events(data: web::Data<AppState>) -> impl Responder {
    let events = data.events.lock().await;
    info!("[HTTP] GET /events - returning {} events", events.len());
    // Detailed log of events for debugging
    for (i, ev) in events.iter().enumerate() {
        info!("[HTTP] Event #{}: {:?}", i, ev);
    }
    HttpResponse::Ok().json(&*events)
}

async fn attempt_reconnection(manager: &Manager) -> Result<(), AmiError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match manager.connect_and_login().await {
            Ok(_) => {
                info!("[MAIN] AMI login successful on attempt {}", attempts);
                return Ok(());
            }
            Err(e) => {
                error!("[MAIN] AMI login failed: {} (attempt {})", e, attempts);
                match &e {
                    AmiError::Timeout | AmiError::ConnectionClosed | AmiError::Io(_)
                        if attempts < RETRY_LIMIT =>
                    {
                        info!("[MAIN] Retrying AMI connection in 2 seconds...");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    _ => {
                        return Err(e);
                    }
                }
            }
        }
    }
}

async fn ensure_manager_connected(app_state: &AppState) -> Result<(), AmiError> {
    let mut attempts = 0;
    loop {
        let mut manager_guard = app_state.manager.lock().await;
        if manager_guard.is_authenticated().await {
            return Ok(());
        }
        attempts += 1;
        println!(
            "[RECONNECT] Trying to reconnect to AMI... attempt {}",
            attempts
        );
        let new_manager = Manager::new(app_state.manager_options.clone());
        match new_manager.connect_and_login().await {
            Ok(_) => {
                *manager_guard = new_manager;
                println!("[RECONNECT] Reconnection successful!");
                return Ok(());
            }
            Err(e) => {
                println!("[RECONNECT] Failed to reconnect: {}", e);
                match &e {
                    AmiError::Timeout | AmiError::ConnectionClosed | AmiError::Io(_)
                        if attempts < RETRY_LIMIT =>
                    {
                        println!("[RECONNECT] Retrying in 2 seconds...");
                        drop(manager_guard); // Release the lock before sleeping
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    _ => {
                        return Err(e);
                    }
                }
            }
        }
    }
}

async fn post_action(data: web::Data<AppState>, req: web::Json<ActionRequest>) -> impl Responder {
    if let Err(e) = ensure_manager_connected(&data).await {
        return HttpResponse::InternalServerError().json(ActionResponse {
            result: "error".into(),
            response: None,
            error: Some(e.to_string()),
        });
    }
    let manager = data.manager.lock().await;
    let action = AmiAction::Custom {
        action: req.action.clone(),
        params: req.params.clone().unwrap_or_default(),
        action_id: None,
    };
    match manager.send_action(action).await {
        Ok(resp) => HttpResponse::Ok().json(ActionResponse {
            result: "ok".into(),
            response: Some(resp),
            error: None,
        }),
        Err(e) => HttpResponse::InternalServerError().json(ActionResponse {
            result: "error".into(),
            response: None,
            error: Some(e.to_string()),
        }),
    }
}

async fn get_calls(data: web::Data<AppState>) -> impl Responder {
    println!("[HTTP] GET /calls - waiting for call events");
    if let Err(e) = ensure_manager_connected(&data).await {
        return HttpResponse::InternalServerError().body(format!("Error reconnecting: {}", e));
    }
    // Send the CoreShowChannels action
    {
        let manager = data.manager.lock().await;
        let action = AmiAction::Custom {
            action: "CoreShowChannels".to_string(),
            params: std::collections::HashMap::new(),
            action_id: None,
        };
        if let Err(e) = manager.send_action(action).await {
            return HttpResponse::InternalServerError()
                .body(format!("Error sending action: {}", e));
        }
    }

    // Wait up to 2 seconds for CoreShowChannel events
    let mut calls = Vec::new();
    let mut last_events_len = 0;
    if let Ok(_) = tokio::time::timeout(Duration::from_secs(2), async {
        let mut no_new_events_count = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let events = data.events.lock().await;
            let new_events = events.len() - last_events_len;
            if new_events == 0 {
                no_new_events_count += 1;
                if no_new_events_count >= 5 {
                    // If no new events arrive in 5 iterations (~500ms), break the loop
                    break;
                }
            } else {
                no_new_events_count = 0;
            }
            // Only process new events since the last iteration
            for ev in events.iter().skip(last_events_len) {
                match ev {
                    AmiEvent::UnknownEvent { fields, .. } => {
                        if let Some(event_type) = fields.get("Event") {
                            if event_type == "CoreShowChannel" {
                                match serde_json::to_value(fields) {
                                    Ok(value) => calls.push(value),
                                    Err(e) => {
                                        println!("[ERROR] Failed to serialize fields: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            last_events_len = events.len();
        }
    })
    .await
    {
        // Timeout occurred, exit the loop
    }
    HttpResponse::Ok().json(CallsResponse { calls })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Configure AMI as needed
    let options = ManagerOptions {
        port: std::env::var("AMI_PORT")
            .unwrap_or_else(|_| "5038".to_string())
            .parse()
            .unwrap_or(5038),
        host: std::env::var("AMI_HOST").unwrap_or_else(|_| "example.voip.net".to_string()),
        username: std::env::var("AMI_USERNAME").unwrap_or_else(|_| "admin".to_string()),
        password: std::env::var("AMI_PASSWORD").unwrap_or_else(|_| "test".to_string()),
        events: true,
    };

    let manager = Manager::new(options.clone());

    // Try to connect and login, with reconnection attempts
    if let Err(e) = attempt_reconnection(&manager).await {
        eprintln!("[MAIN] AMI login failed after multiple attempts: {}", e);
        std::process::exit(1);
    }
    println!("[MAIN] AMI login successful.");

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut event_stream = manager.all_events_stream().await;

    let app_state = AppState {
        manager: Arc::new(Mutex::new(manager)),
        events,
        manager_options: options.clone(),
    };

    // Task to collect events in memory and trigger reconnection if needed
    let app_state_clone = app_state.clone();
    tokio::spawn(async move {
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    println!("[AMI EVENT] RECEIVED: {:?}", event);
                    match &event {
                        AmiEvent::UnknownEvent { event_type, fields } => {
                            println!(
                                "[AMI EVENT] UnknownEvent: event_type={}, fields={:?}",
                                event_type, fields
                            );
                        }
                        AmiEvent::Newchannel(fields) => {
                            println!("[AMI EVENT] Newchannel: {:?}", fields);
                        }
                        AmiEvent::Hangup(fields) => {
                            println!("[AMI EVENT] Hangup: {:?}", fields);
                        }
                        AmiEvent::PeerStatus(fields) => {
                            println!("[AMI EVENT] PeerStatus: {:?}", fields);
                        }
                        AmiEvent::InternalConnectionLost { error } => {
                            println!(
                                "[AMI EVENT] Other: InternalConnectionLost {{ error: {:?} }}",
                                error
                            );
                            let app_state_reconnect = app_state_clone.clone();
                            tokio::spawn(async move {
                                if let Err(e) = ensure_manager_connected(&app_state_reconnect).await
                                {
                                    eprintln!(
                                        "[RECONNECT] Failed to automatically reconnect: {}",
                                        e
                                    );
                                }
                            });
                        }
                    }
                    let mut evs = app_state_clone.events.lock().await;
                    evs.push(event);
                    let len = evs.len();
                    if len > MAX_EVENT_BUFFER_SIZE {
                        println!(
                            "[AMI EVENT] Event limit reached, discarding old ones ({} removed)",
                            len - MAX_EVENT_BUFFER_SIZE
                        );
                        evs.drain(0..len - MAX_EVENT_BUFFER_SIZE);
                    }
                }
                Err(e) => {
                    eprintln!("[AMI ERROR] Failed to receive event from stream: {:?}", e);
                }
            }
        }
        println!("[AMI EVENT] Event stream finished.");
    });

    println!("Actix Web server running at http://127.0.0.1:8080");
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .route("/events", web::get().to(get_events))
            .route("/action", web::post().to(post_action))
            .route("/calls", web::get().to(get_calls))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
