use asterisk_manager::resilient::{connect_resilient, ResilientOptions};
use asterisk_manager::ManagerOptions;
use chrono::Local;
use env_logger::{Builder, Env};
use log::info;
use std::io::Write;

// Example demonstrating multiple Manager instances with watchdog and heartbeat.
// Each Manager has a unique instance_id visible in logs, making it easy to
// distinguish between different connections when running multiple managers.
//
// Run with:
//   RUST_LOG=info,asterisk_manager=debug \
//   cargo run --example multiple_managers_logging

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    // Configure logger
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

    info!("=== Starting Multiple Manager Instances Example ===");
    info!("Each manager will have a unique [instance_id] in logs");
    info!("");

    // Optional: Configure via .env file:
    //   # First manager
    //   AMI_HOST_1=127.0.0.1
    //   AMI_PORT_1=5038
    //   AMI_USERNAME_1=admin
    //   AMI_PASSWORD_1=password
    //   # Second manager
    //   AMI_HOST_2=127.0.0.1
    //   AMI_PORT_2=5039
    //   AMI_USERNAME_2=admin2
    //   AMI_PASSWORD_2=password2

    // Configuration for first manager
    let options1 = ResilientOptions {
        manager_options: ManagerOptions {
            port: std::env::var("AMI_PORT_1")
                .unwrap_or_else(|_| "5038".to_string())
                .parse()
                .unwrap_or(5038),
            host: std::env::var("AMI_HOST_1").unwrap_or_else(|_| "127.0.0.1".to_string()),
            username: std::env::var("AMI_USERNAME_1").unwrap_or_else(|_| "admin".to_string()),
            password: std::env::var("AMI_PASSWORD_1").unwrap_or_else(|_| "password".to_string()),
            events: true,
        },
        buffer_size: 2048,
        enable_heartbeat: true,
        enable_watchdog: true,
        heartbeat_interval: 10, // Short interval for demo
        watchdog_interval: 2,   // Short interval for demo
        max_retries: 3,
        metrics: None,
        cumulative_attempts_counter: None,
    };

    // Configuration for second manager (could be a different server)
    let options2 = ResilientOptions {
        manager_options: ManagerOptions {
            port: std::env::var("AMI_PORT_2")
                .unwrap_or_else(|_| "5039".to_string())
                .parse()
                .unwrap_or(5039),
            host: std::env::var("AMI_HOST_2").unwrap_or_else(|_| "127.0.0.1".to_string()),
            username: std::env::var("AMI_USERNAME_2").unwrap_or_else(|_| "admin2".to_string()),
            password: std::env::var("AMI_PASSWORD_2")
                .unwrap_or_else(|_| "password2".to_string()),
            events: true,
        },
        buffer_size: 2048,
        enable_heartbeat: true,
        enable_watchdog: true,
        heartbeat_interval: 15, // Different interval
        watchdog_interval: 3,   // Different interval
        max_retries: 5,
        metrics: None,
        cumulative_attempts_counter: None,
    };

    info!("Creating first manager connection...");
    let manager1 = match connect_resilient(options1).await {
        Ok(m) => {
            info!("✓ First manager connected successfully");
            Some(m)
        }
        Err(e) => {
            info!("✗ First manager failed to connect: {e}");
            info!("  (watchdog will keep trying to reconnect)");
            None
        }
    };

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    info!("");
    info!("Creating second manager connection...");
    let manager2 = match connect_resilient(options2).await {
        Ok(m) => {
            info!("✓ Second manager connected successfully");
            Some(m)
        }
        Err(e) => {
            info!("✗ Second manager failed to connect: {e}");
            info!("  (watchdog will keep trying to reconnect)");
            None
        }
    };

    info!("");
    info!("=== Both managers are now running ===");
    info!("Watch the logs to see [instance_id] for each manager:");
    info!("  - Heartbeat pings every 10s (manager1) and 15s (manager2)");
    info!("  - Watchdog checks every 2s (manager1) and 3s (manager2)");
    info!("");
    info!("Set RUST_LOG=asterisk_manager=debug to see detailed logs.");
    info!("Press Ctrl+C to exit.");

    // Periodic status check
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let auth1 = if let Some(ref m) = manager1 {
                m.is_authenticated().await
            } else {
                false
            };
            let auth2 = if let Some(ref m) = manager2 {
                m.is_authenticated().await
            } else {
                false
            };
            info!(
                "Status check - Manager1: {}, Manager2: {}",
                if auth1 { "✓ authenticated" } else { "✗ not authenticated" },
                if auth2 { "✓ authenticated" } else { "✗ not authenticated" }
            );
        }
    });

    // Keep the process alive
    tokio::signal::ctrl_c().await?;
    info!("Shutting down example.");
    Ok(())
}
