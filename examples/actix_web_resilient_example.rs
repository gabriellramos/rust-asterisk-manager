use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use asterisk_manager::{AmiAction, AmiEvent, AmiResponse, Manager, ManagerOptions};
use asterisk_manager::resilient::{ResilientOptions, connect_resilient, infinite_events_stream};
use chrono::Local;
use env_logger::{Builder, Env};
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio_stream::StreamExt;
use uuid::Uuid;

const MAX_EVENT_BUFFER_SIZE: usize = 1000;

#[derive(Clone)]
struct AppState {
    manager: Arc<Mutex<Manager>>,
    events: Arc<Mutex<Vec<AmiEvent>>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct CallsResponse {
    calls: Vec<serde_json::Value>,
}

async fn get_events(data: web::Data<AppState>) -> impl Responder {
    let events = data.events.lock().await;
    info!("[HTTP] GET /events - returning {} events", events.len());
    HttpResponse::Ok().json(&*events)
}

async fn post_action(data: web::Data<AppState>, req: web::Json<ActionRequest>) -> impl Responder {
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

fn get_fields_if_action_id_matches<'a>(
    event: &'a AmiEvent,
    expected_action_id: &str,
) -> Option<&'a std::collections::HashMap<String, String>> {
    // Most response events to an action will come as UnknownEvent
    // because they do not have a specific type in the AmiEvent enum.
    if let AmiEvent::UnknownEvent { fields, .. } = event {
        if fields
            .get("ActionID")
            .map_or(false, |id| id == expected_action_id)
        {
            return Some(fields);
        }
    }
    // In the future, if other event types in the AmiEvent enum (e.g., PeerStatus)
    // can contain an ActionID, they can be added here.
    // Example:
    // if let AmiEvent::PeerStatus(data) = event {
    //     if data.other.get("ActionID").map_or(false, |id| id == expected_action_id) {
    //         return Some(&data.other);
    //     }
    // }
    None
}

async fn get_calls(data: web::Data<AppState>) -> impl Responder {
    info!("[HTTP] GET /calls - requesting list of active calls");

    let action_id = Uuid::new_v4().to_string();

    {
        let manager = data.manager.lock().await;
        let action = AmiAction::Custom {
            action: "CoreShowChannels".to_string(),
            params: std::collections::HashMap::new(),
            action_id: Some(action_id.clone()),
        };

        match manager.send_action(action).await {
            Ok(resp) if resp.response.eq_ignore_ascii_case("Success") => {
                info!("[HTTP] GET /calls - CoreShowChannels action sent successfully.");
            }
            Ok(resp) => {
                let err_msg = format!("CoreShowChannels action failed with response: {:?}", resp);
                error!("[HTTP] GET /calls - {}", err_msg);
                return HttpResponse::InternalServerError().body(err_msg);
            }
            Err(e) => {
                let err_msg = format!("Error sending CoreShowChannels action: {}", e);
                error!("[HTTP] GET /calls - {}", err_msg);
                return HttpResponse::InternalServerError().body(err_msg);
            }
        }
    }

    let mut calls = Vec::new();

    if tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let events_guard = data.events.lock().await;
            let mut found_complete = false;

            for ev in events_guard.iter() {
                if let Some(fields) = get_fields_if_action_id_matches(ev, &action_id) {
                    let event_type = fields.get("Event").map(|s| s.as_str());

                    if event_type == Some("CoreShowChannel") {
                        if let Ok(value) = serde_json::to_value(fields) {
                            if let Some(uid) = value.get("Uniqueid") {
                                let is_duplicate = calls.iter().any(|c: &Value| {
                                    c.get("Uniqueid")
                                        .map_or(false, |existing_uid| existing_uid == uid)
                                });

                                if !is_duplicate {
                                    calls.push(value);
                                }
                            }
                        }
                    } else if event_type == Some("CoreShowChannelsComplete") {
                        info!("[HTTP] GET /calls - 'CoreShowChannelsComplete' event received.");
                        found_complete = true;
                    }
                }
            }
            drop(events_guard);

            if found_complete {
                break;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .is_err()
    {
        error!("[HTTP] GET /calls - Timeout occurred waiting for CoreShowChannelsComplete.");
    }

    info!("[HTTP] GET /calls - Returning {} calls.", calls.len());
    HttpResponse::Ok().json(CallsResponse { calls })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    Builder::from_env(Env::new().default_filter_or("info"))
        .format(|buf, record| {
            let ts = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            writeln!(
                buf,
                "[{} {} {}] {}",
                ts,
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();

    // Configure resilient AMI connection options
    let resilient_options = ResilientOptions {
        manager_options: ManagerOptions {
            port: std::env::var("AMI_PORT")
                .unwrap_or_else(|_| "5038".to_string())
                .parse()
                .unwrap_or(5038),
            host: std::env::var("AMI_HOST").unwrap_or_else(|_| "localhost".to_string()),
            username: std::env::var("AMI_USERNAME").unwrap_or_else(|_| "admin".to_string()),
            password: std::env::var("AMI_PASSWORD").unwrap_or_else(|_| "password".to_string()),
            events: true,
        },
        buffer_size: 2048,
        enable_heartbeat: true,
        enable_watchdog: true,
        heartbeat_interval: 30,
        watchdog_interval: 1,
    };

    // Connect using resilient connection - automatic reconnection handled internally
    let manager = match connect_resilient(resilient_options.clone()).await {
        Ok(manager) => {
            info!("Successfully connected to AMI with resilient features enabled");
            manager
        }
        Err(e) => {
            error!("Failed to connect to AMI: {}. The application will continue and the resilient connection will keep trying to reconnect.", e);
            // Even if initial connection fails, the resilient system will keep trying
            match connect_resilient(resilient_options.clone()).await {
                Ok(manager) => manager,
                Err(_) => {
                    // Create a disconnected manager - resilient system will handle reconnection
                    Manager::new_with_buffer(2048)
                }
            }
        }
    };

    let app_state = AppState {
        manager: Arc::new(Mutex::new(manager)),
        events: Arc::new(Mutex::new(Vec::new())),
    };

    //==================================================================================//
    // EVENT COLLECTION TASK - USING RESILIENT INFINITE STREAM
    //==================================================================================//
    let app_state_for_task = app_state.clone();
    tokio::spawn(async move {
        info!("[EVENT_TASK] Starting resilient event collection...");

        // Create infinite event stream that handles all reconnection automatically
        let event_stream = match infinite_events_stream(resilient_options).await {
            Ok(stream) => stream,
            Err(e) => {
                error!("[EVENT_TASK] Failed to create infinite event stream: {}", e);
                return;
            }
        };

        tokio::pin!(event_stream);

        info!("[EVENT_TASK] Connected to infinite event stream with automatic reconnection");

        // This stream never ends and handles all reconnection logic internally
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    info!("[AMI_EVENT] Received: {:?}", event);
                    let mut evs = app_state_for_task.events.lock().await;
                    evs.push(event);

                    let len = evs.len();
                    if len > MAX_EVENT_BUFFER_SIZE {
                        let amount_to_drain = len - MAX_EVENT_BUFFER_SIZE;
                        info!(
                            "[EVENT_TASK] Event limit reached, discarding {} old events.",
                            amount_to_drain
                        );
                        evs.drain(0..amount_to_drain);
                    }
                }
                Err(e) => {
                    // The resilient stream handles reconnection, so errors here are just logged
                    error!("[EVENT_TASK] Error in event stream (will auto-recover): {:?}", e);
                }
            }
        }

        // This should never be reached since infinite_events_stream never ends
        error!("[EVENT_TASK] Infinite event stream unexpectedly ended");
    });

    info!("Actix Web server with resilient AMI connections running at http://0.0.0.0:8080");
    info!("Features enabled: automatic reconnection, heartbeat monitoring, connection watchdog");
    
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .route("/events", web::get().to(get_events))
            .route("/action", web::post().to(post_action))
            .route("/calls", web::get().to(get_calls))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}