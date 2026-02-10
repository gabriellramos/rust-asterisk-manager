/// Example demonstrating how to use the Originate action with multiple Variables.
///
/// This example shows how to send an Originate action with multiple Variable parameters,
/// which is now possible since params is a Vec<(String, String)> instead of a HashMap.
///
/// To run this example:
/// ```bash
/// cargo run --example originate_with_variables
/// ```
///
/// Note: This example requires a running Asterisk instance with AMI enabled.
/// Configure your connection details using environment variables:
/// - AMI_HOST (default: localhost)
/// - AMI_PORT (default: 5038)
/// - AMI_USERNAME (default: admin)
/// - AMI_PASSWORD (default: password)

use asterisk_manager::{AmiAction, Manager, ManagerOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let options = ManagerOptions {
        port: std::env::var("AMI_PORT")
            .unwrap_or_else(|_| "5038".to_string())
            .parse()
            .unwrap_or(5038),
        host: std::env::var("AMI_HOST").unwrap_or_else(|_| "localhost".to_string()),
        username: std::env::var("AMI_USERNAME").unwrap_or_else(|_| "admin".to_string()),
        password: std::env::var("AMI_PASSWORD").unwrap_or_else(|_| "password".to_string()),
        events: true,
    };

    let mut manager = Manager::new();

    println!("Connecting to AMI at {}:{}...", options.host, options.port);
    manager.connect_and_login(options).await?;
    println!("Successfully connected and authenticated!");

    // Example 1: Originate action with multiple Variable parameters
    println!("\n--- Example 1: Originate with multiple Variables ---");
    let originate_action = AmiAction::Custom {
        action: "Originate".to_string(),
        params: vec![
            ("Channel".to_string(), "PJSIP/user1".to_string()),
            ("Application".to_string(), "Dial".to_string()),
            ("Data".to_string(), "PJSIP/1234@trunk".to_string()),
            // Multiple Variable parameters - this was not possible with HashMap
            ("Variable".to_string(), "CDR(extra_data)=123".to_string()),
            ("Variable".to_string(), "__ID_EXTRA=456".to_string()),
            ("Variable".to_string(), "__ID_MAIN=789".to_string()),
        ],
        action_id: None,
    };

    println!("Sending Originate action with multiple Variables...");
    match manager.send_action(originate_action).await {
        Ok(response) => {
            println!("Response: {:?}", response);
            if response.response.eq_ignore_ascii_case("Success") {
                println!("Originate action was successful!");
            } else {
                println!("Originate action failed: {}", response.message.unwrap_or_default());
            }
        }
        Err(e) => {
            eprintln!("Error sending action: {}", e);
        }
    }

    // Example 2: Using params as a vector allows preserving order and duplicates
    println!("\n--- Example 2: Originate with Context/Extension/Priority ---");
    let originate_action2 = AmiAction::Custom {
        action: "Originate".to_string(),
        params: vec![
            ("Channel".to_string(), "SIP/100".to_string()),
            ("Context".to_string(), "default".to_string()),
            ("Exten".to_string(), "200".to_string()),
            ("Priority".to_string(), "1".to_string()),
            ("CallerID".to_string(), "1000".to_string()),
            // Additional Variables for this call
            ("Variable".to_string(), "CALL_TYPE=outbound".to_string()),
            ("Variable".to_string(), "CAMPAIGN_ID=summer2024".to_string()),
        ],
        action_id: None,
    };

    println!("Sending second Originate action...");
    match manager.send_action(originate_action2).await {
        Ok(response) => {
            println!("Response: {:?}", response);
        }
        Err(e) => {
            eprintln!("Error sending action: {}", e);
        }
    }

    manager.disconnect().await?;
    println!("\nDisconnected from AMI.");
    Ok(())
}
