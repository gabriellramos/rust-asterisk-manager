use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use asterisk_manager::resilient::{
    connect_resilient, infinite_events_stream, ResilientMetrics, ResilientOptions,
};
use asterisk_manager::{AmiAction, AmiEvent, AmiResponse, Manager, ManagerOptions};
use chrono::Local;
use env_logger::{Builder, Env};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use tokio_stream::StreamExt;
use uuid::Uuid;

const MAX_EVENT_BUFFER_SIZE: usize = 1000;

#[derive(Clone)]
struct AppState {
    manager: Arc<Mutex<Manager>>,
    events: Arc<Mutex<Vec<AmiEvent>>>,
    // Resilient connection metrics for monitoring
    metrics: Arc<ResilientMetrics>,
    // Application-level metrics
    connection_status: Arc<Mutex<ConnectionStatus>>,
    start_time: Instant,
}

/// Connection status tracking for the application
#[derive(Debug, Clone)]
struct ConnectionStatus {
    is_connected: bool,
    last_connection_attempt: Option<Instant>,
    last_successful_connection: Option<Instant>,
    total_uptime: Duration,
    total_downtime: Duration,
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
            .is_some_and(|id| id == expected_action_id)
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
                                    c.get("Uniqueid") == Some(uid)
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

/// Get detailed metrics about the resilient connection
async fn get_metrics(data: web::Data<AppState>) -> impl Responder {
    let metrics = data.metrics.snapshot();
    let connection_status = data.connection_status.lock().await;
    let uptime = data.start_time.elapsed();

    let response = serde_json::json!({
        "resilient_metrics": {
            "reconnection_attempts": metrics.0,
            "successful_reconnections": metrics.1,
            "failed_reconnections": metrics.2,
            "connection_lost_events": metrics.3,
            "last_reconnection_duration_ms": metrics.4,
        },
        "connection_status": {
            "is_connected": connection_status.is_connected,
            "last_connection_attempt": connection_status.last_connection_attempt
                .map(|t| t.elapsed().as_secs()),
            "last_successful_connection": connection_status.last_successful_connection
                .map(|t| t.elapsed().as_secs()),
            "total_uptime_seconds": connection_status.total_uptime.as_secs(),
            "total_downtime_seconds": connection_status.total_downtime.as_secs(),
        },
        "application_metrics": {
            "total_uptime_seconds": uptime.as_secs(),
            "events_in_buffer": data.events.lock().await.len(),
            "buffer_utilization_percent": (data.events.lock().await.len() as f64 / MAX_EVENT_BUFFER_SIZE as f64 * 100.0),
        }
    });

    info!("[HTTP] GET /metrics - returning connection and resilience metrics");
    HttpResponse::Ok().json(response)
}

/// Get connection health check
async fn health_check(data: web::Data<AppState>) -> impl Responder {
    let connection_status = data.connection_status.lock().await;
    let metrics = data.metrics.snapshot();

    let is_healthy = connection_status.is_connected && (metrics.3 == 0 || metrics.1 > 0); // No connection losses or successful reconnections

    let status_code = if is_healthy { 200 } else { 503 };
    let status = if is_healthy { "healthy" } else { "degraded" };

    let response = serde_json::json!({
        "status": status,
        "is_connected": connection_status.is_connected,
        "reconnection_attempts": metrics.0,
        "connection_lost_events": metrics.3,
    });

    HttpResponse::build(actix_web::http::StatusCode::from_u16(status_code).unwrap()).json(response)
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

    // Configure resilient AMI connection options with full monitoring capabilities
    let metrics = Arc::new(ResilientMetrics::new());
    let global_counter = Arc::new(AtomicU64::new(0));

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
        buffer_size: 4096, // Larger buffer for high-traffic scenarios
        enable_heartbeat: true,
        enable_watchdog: true,
        heartbeat_interval: std::env::var("AMI_HEARTBEAT_INTERVAL")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30),
        watchdog_interval: 1,
        max_retries: 10, // Higher retry count for production resilience
        metrics: Some((*metrics).clone()),
        cumulative_attempts_counter: Some(global_counter.clone()),
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
        metrics: metrics.clone(),
        connection_status: Arc::new(Mutex::new(ConnectionStatus {
            is_connected: true,
            last_connection_attempt: Some(Instant::now()),
            last_successful_connection: Some(Instant::now()),
            total_uptime: Duration::from_secs(0),
            total_downtime: Duration::from_secs(0),
        })),
        start_time: Instant::now(),
    };

    //==================================================================================//
    // EVENT COLLECTION TASK - USING RESILIENT INFINITE STREAM WITH FULL MONITORING
    //==================================================================================//
    let app_state_for_task = app_state.clone();
    let resilient_options_for_task = resilient_options.clone();
    tokio::spawn(async move {
        info!("[EVENT_TASK] Starting resilient event collection with full monitoring...");

        // Create infinite event stream that handles all reconnection automatically
        let event_stream = match infinite_events_stream(resilient_options_for_task).await {
            Ok(stream) => stream,
            Err(e) => {
                error!("[EVENT_TASK] Failed to create infinite event stream: {}", e);
                return;
            }
        };

        tokio::pin!(event_stream);

        info!("[EVENT_TASK] Connected to infinite event stream with automatic reconnection and metrics");

        let mut last_connection_check = Instant::now();
        let mut was_connected = true;
        let mut downtime_start: Option<Instant> = None;

        // This stream never ends and handles all reconnection logic internally
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    // Monitor connection status based on events
                    match &event {
                        AmiEvent::InternalConnectionLost { .. } => {
                            warn!("[EVENT_TASK] Connection lost detected - updating status");
                            let mut status = app_state_for_task.connection_status.lock().await;
                            status.is_connected = false;
                            status.last_connection_attempt = Some(Instant::now());
                            if was_connected {
                                downtime_start = Some(Instant::now());
                                was_connected = false;
                            }
                        }
                        _ => {
                            // Any other event means we're connected
                            if !was_connected {
                                info!("[EVENT_TASK] Connection restored - updating status");
                                let mut status = app_state_for_task.connection_status.lock().await;
                                status.is_connected = true;
                                status.last_successful_connection = Some(Instant::now());
                                if let Some(start) = downtime_start {
                                    status.total_downtime += start.elapsed();
                                    downtime_start = None;
                                }
                                was_connected = true;
                            }
                        }
                    }

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

                    // Periodic connection status logging
                    if last_connection_check.elapsed() > Duration::from_secs(60) {
                        let metrics = app_state_for_task.metrics.snapshot();
                        info!(
                            "[EVENT_TASK] Periodic status - Reconnections: {}, Lost: {}, Buffer: {}/{}",
                            metrics.1, metrics.3, len, MAX_EVENT_BUFFER_SIZE
                        );
                        last_connection_check = Instant::now();
                    }
                }
                Err(e) => {
                    // The resilient stream handles reconnection, so errors here are just logged
                    error!(
                        "[EVENT_TASK] Error in event stream (will auto-recover): {:?}",
                        e
                    );
                }
            }
        }

        // This should never be reached since infinite_events_stream never ends
        error!("[EVENT_TASK] Infinite event stream unexpectedly ended");
    });

    info!("Actix Web server with resilient AMI connections running at http://0.0.0.0:8080");
    info!("Features enabled: automatic reconnection, heartbeat monitoring, connection watchdog, metrics collection");
    info!("Available endpoints:");
    info!("  GET  /events  - Get recent AMI events");
    info!("  POST /action  - Send AMI action");
    info!("  GET  /calls   - Get active calls");
    info!("  GET  /metrics - Get resilient connection metrics");
    info!("  GET  /health  - Health check endpoint");
    info!("Environment variables:");
    info!(
        "  AMI_HOST={}",
        std::env::var("AMI_HOST").unwrap_or_else(|_| "localhost".to_string())
    );
    info!(
        "  AMI_PORT={}",
        std::env::var("AMI_PORT").unwrap_or_else(|_| "5038".to_string())
    );
    info!(
        "  AMI_HEARTBEAT_INTERVAL={}",
        std::env::var("AMI_HEARTBEAT_INTERVAL").unwrap_or_else(|_| "30".to_string())
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .route("/events", web::get().to(get_events))
            .route("/action", web::post().to(post_action))
            .route("/calls", web::get().to(get_calls))
            .route("/metrics", web::get().to(get_metrics))
            .route("/health", web::get().to(health_check))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
