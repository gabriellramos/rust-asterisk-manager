use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use asterisk_manager::{AmiAction, AmiEvent, AmiResponse, Manager, ManagerOptions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio_stream::StreamExt;

#[derive(Clone)]
struct AppState {
    manager: Arc<Mutex<Manager>>,
    events: Arc<Mutex<Vec<AmiEvent>>>,
    manager_options: Arc<ManagerOptions>,
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
    println!("[HTTP] GET /events - returning {} events", events.len());
    // Detailed log of events for debugging
    for (i, ev) in events.iter().enumerate() {
        println!("[HTTP] Event #{}: {:?}", i, ev);
    }
    HttpResponse::Ok().json(&*events)
}

async fn ensure_manager_connected(app_state: &AppState) -> Result<(), String> {
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
        let new_manager = Manager::new((*app_state.manager_options).clone());
        match new_manager.connect_and_login().await {
            Ok(_) => {
                *manager_guard = new_manager;
                println!("[RECONNECT] Reconnection successful!");
                return Ok(());
            }
            Err(e) => {
                println!("[RECONNECT] Failed to reconnect: {}", e);
                let err_str = format!("{}", e);
                if (err_str.contains("Timeout")
                    || err_str.contains("Operation timed out")
                    || err_str.contains("Connection refused")
                    || err_str.contains("IO error"))
                    && attempts < 100
                {
                    println!("[RECONNECT] Retrying in 2 seconds...");
                    drop(manager_guard); // Release the lock before sleeping
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                } else {
                    return Err(format!("Failed to reconnect: {}", e));
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
            error: Some(e),
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
    let start = std::time::Instant::now();
    loop {
        // Wait a short interval to ensure events have arrived
        tokio::time::sleep(Duration::from_millis(100)).await;
        let events = data.events.lock().await;
        for ev in events.iter() {
            match ev {
                AmiEvent::UnknownEvent { fields, .. } => {
                    if let Some(event_type) = fields.get("Event") {
                        if event_type == "CoreShowChannel" {
                            calls.push(serde_json::to_value(fields).unwrap_or_default());
                        }
                    }
                }
                _ => {}
            }
        }
        // Exit the loop after 2 seconds
        if start.elapsed() > Duration::from_secs(2) {
            break;
        }
    }

    HttpResponse::Ok().json(CallsResponse { calls })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Configure AMI as needed
    let options = ManagerOptions {
        port: 5038,
        host: "example.voip.net".to_string(),
        username: "admin".to_string(),
        password: "test".to_string(),
        events: true,
    };

    let manager = Manager::new(options.clone());

    // Try to connect and login, with reconnection attempts on Timeout, Operation timed out, Connection refused or IO error
    let mut attempts = 0;
    loop {
        attempts += 1;
        match manager.connect_and_login().await {
            Ok(_) => {
                println!("[MAIN] AMI login successful on attempt {}", attempts);
                break;
            }
            Err(e) => {
                eprintln!("[MAIN] AMI login failed: {} (attempt {})", e, attempts);
                let err_str = format!("{}", e);
                if (err_str.contains("Timeout")
                    || err_str.contains("Operation timed out")
                    || err_str.contains("Connection refused")
                    || err_str.contains("IO error"))
                    && attempts < 100
                {
                    println!("[MAIN] Retrying AMI connection in 2 seconds...");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                } else {
                    panic!("AMI login failed: {}", e);
                }
            }
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut event_stream = manager.all_events_stream().await;

    let app_state = AppState {
        manager: Arc::new(Mutex::new(manager)),
        events,
        manager_options: Arc::new(options),
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
                    if len > 1000 {
                        println!(
                            "[AMI EVENT] Event limit reached, discarding old ones ({} removed)",
                            len - 1000
                        );
                        evs.drain(0..len - 1000);
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
