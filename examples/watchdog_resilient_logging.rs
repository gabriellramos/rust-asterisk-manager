use asterisk_manager::{Manager, ManagerOptions};
use chrono::Local;
use env_logger::{Builder, Env};
use log::{info, warn};
use std::io::Write;

// A minimal example to demonstrate the built-in watchdog reconnection task
// and how to see its logs. The watchdog runs periodically and, when the
// manager is not authenticated, it attempts to connect_and_login using the
// provided ManagerOptions. Its internal logs include:
//   - "Watchdog attempting reconnection..." (debug)
//   - "Watchdog reconnection successful" (info)
//   - "Watchdog reconnection failed: <error>" (debug)
// To see these messages, run with RUST_LOG including `asterisk_manager=debug`.
//
// Example run:
//   RUST_LOG=info,asterisk_manager=debug \
//   cargo run --example watchdog_resilient_logging

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    // Configure logger: default to "info" but allow overriding via RUST_LOG
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

    // Read AMI connection info from env vars or use defaults
    let options = ManagerOptions {
        port: std::env::var("AMI_PORT")
            .unwrap_or_else(|_| "5038".to_string())
            .parse()
            .unwrap_or(5038),
        host: std::env::var("AMI_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        username: std::env::var("AMI_USERNAME").unwrap_or_else(|_| "admin".to_string()),
        password: std::env::var("AMI_PASSWORD").unwrap_or_else(|_| "password".to_string()),
        events: true,
    };

    // Start with a fresh manager (not authenticated yet)
    let manager = Manager::new();

    // Start only the watchdog with a short interval (in seconds).
    // The watchdog will try to connect until success and will log attempts.
    manager
        .start_watchdog_with_interval(options.clone(), 2)
        .await?;

    info!(
        "Watchdog started (interval=2s). Set RUST_LOG=asterisk_manager=debug to see detailed attempts."
    );
    info!(
        "Environment: AMI_HOST={}, AMI_PORT={}",
        options.host, options.port
    );

    // Optional: observe authentication status every 5 seconds
    let manager_clone = manager.clone();
    tokio::spawn(async move {
        loop {
            let auth = manager_clone.is_authenticated().await;
            if auth {
                info!("Status: authenticated to AMI");
            } else {
                warn!("Status: not authenticated (watchdog will attempt reconnect)");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });

    // Keep the process alive so we can observe the watchdog behavior.
    // Ctrl+C to exit.
    tokio::signal::ctrl_c().await?;
    info!("Shutting down example.");
    Ok(())
}
