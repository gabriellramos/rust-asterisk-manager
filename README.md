# Asterisk Manager Library (v1.0.0)

Esta biblioteca fornece uma implementação moderna, fortemente tipada e baseada em streams para interação com o Asterisk Manager Interface (AMI).

## Features

- **Mensagens AMI tipadas**: Actions, Events e Responses como enums/structs Rust.
- **API baseada em Stream**: Consumo de eventos via tokio_stream.
- **Operações assíncronas**: Utiliza Tokio.
- **Reconexão automática** e **correlação ActionID/Response**.

## Exemplo de uso

```rust,no_run
use asterisk_manager::{Manager, ManagerOptions, AmiAction};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let options = ManagerOptions {
        port: 5038,
        host: "127.0.0.1".to_string(),
        username: "admin".to_string(),
        password: "password".to_string(),
        events: true,
    };
    let mut manager = Manager::new(options);
    manager.connect_and_login().await.unwrap();
    let mut events = manager.all_events_stream();
    tokio::spawn(async move {
        while let Some(ev) = events.next().await {
            println!("Evento: {:?}", ev);
        }
    });
    let resp = manager.send_action(AmiAction::Ping { action_id: None }).await.unwrap();
    println!("Resposta ao Ping: {:?}", resp);
    manager.disconnect().await.unwrap();
}
```

## Instalação

Adicione ao seu `Cargo.toml`:

```toml
[dependencies]
asterisk-manager = "1.0.0"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1", features = ["v4"] }
log = "0.4"
```

## Baseado em

Inspirado por [NodeJS-AsteriskManager](https://github.com/pipobscure/NodeJS-AsteriskManager) e [node-asterisk](https://github.com/mscdex/node-asterisk), mas com foco em Rust moderno e tipagem forte.
