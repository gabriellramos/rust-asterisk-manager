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
    println!("[HTTP] GET /events - retornando {} eventos", events.len());
    HttpResponse::Ok().json(&*events)
}

async fn post_action(data: web::Data<AppState>, req: web::Json<ActionRequest>) -> impl Responder {
    let mut manager = data.manager.lock().await;
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
    println!("[HTTP] GET /calls - aguardando eventos de chamadas");
    // Envia a action CoreShowChannels
    {
        let mut manager = data.manager.lock().await;
        let action = AmiAction::Custom {
            action: "CoreShowChannels".to_string(),
            params: std::collections::HashMap::new(),
            action_id: None,
        };
        if let Err(e) = manager.send_action(action).await {
            return HttpResponse::InternalServerError()
                .body(format!("Erro ao enviar action: {}", e));
        }
    }

    // Aguarda até 2 segundos por eventos CoreShowChannel
    let mut calls = Vec::new();
    let start = std::time::Instant::now();
    loop {
        // Aguarda um pequeno intervalo para garantir que os eventos chegaram
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
        // Sai do loop após 2 segundos
        if start.elapsed() > Duration::from_secs(2) {
            break;
        }
    }

    HttpResponse::Ok().json(CallsResponse { calls })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Configure o AMI conforme necessário
    let options = ManagerOptions {
        port: 5038,
        host: "example.voip.net".to_string(),
        username: "admin".to_string(),
        password: "test".to_string(),
        events: true,
    };

    let mut manager = Manager::new(options);
    manager.connect_and_login().await.expect("AMI login failed");

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let mut event_stream = manager.all_events_stream();

    // Task para coletar eventos em memória
    tokio::spawn(async move {
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
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
                        _ => {
                            println!("[AMI EVENT] Outro: {:?}", event);
                        }
                    }
                    let mut evs = events_clone.lock().await;
                    evs.push(event);
                    let len = evs.len();
                    if len > 1000 {
                        println!("[AMI EVENT] Limite de eventos atingido, descartando antigos ({} removidos)", len - 1000);
                        evs.drain(0..len - 1000);
                    }
                }
                Err(e) => {
                    eprintln!("[AMI ERROR] Falha ao receber evento do stream: {:?}", e);
                }
            }
        }
        println!("[AMI EVENT] Stream de eventos finalizado.");
    });

    let app_state = AppState {
        manager: Arc::new(Mutex::new(manager)),
        events,
    };

    println!("Servidor Actix Web rodando em http://127.0.0.1:8080");
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
