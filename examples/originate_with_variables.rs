/// Example demonstrating how to use the Originate action with multiple Variables.
///
/// This example shows how to send an Originate action with multiple Variable parameters.
/// It demonstrates both the new typed AmiAction::Originate variant and the
/// flexible AmiAction::Custom variant with Vec<(String, String)> for params.
///
/// The new Originate variant provides:
/// - Type safety with explicit fields
/// - HashMap<String, Vec<String>> for variables supporting:
///   - Multiple variables with individual values
///   - Each variable can have multiple values
/// - Automatic validation of required fields
/// - Automatic validation of keys and values
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
use std::collections::HashMap;

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

    // Example 1: Using the new typed Originate action with Application/Data
    println!("\n--- Example 1: Originate with Application (typed variant) ---");
    
    let mut variables = HashMap::new();
    // Each variable can have multiple values
    variables.insert("CDR(extra_data)".to_string(), vec!["123".to_string()]);
    variables.insert("__ID_EXTRA".to_string(), vec!["456".to_string()]);
    variables.insert("__ID_MAIN".to_string(), vec!["789".to_string()]);
    // Example of a variable with multiple values
    variables.insert("TAGS".to_string(), vec!["tag1".to_string(), "tag2".to_string()]);

    let originate_action = AmiAction::Originate {
        channel: "PJSIP/user1".to_string(),
        application: Some("Dial".to_string()),
        data: Some("PJSIP/1234@trunk".to_string()),
        timeout: Some(30000), // 30 seconds
        caller_id: Some("1000".to_string()),
        context: None,
        exten: None,
        priority: None,
        variables: Some(variables),
        action_id: None,
    };

    // Validation is automatically performed when sending the action
    println!("Sending typed Originate action with variables...");
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

    // Example 2: Using the typed Originate action with Context/Extension/Priority
    println!("\n--- Example 2: Originate with Context/Extension (typed variant) ---");
    
    let mut variables2 = HashMap::new();
    variables2.insert("CALL_TYPE".to_string(), vec!["outbound".to_string()]);
    variables2.insert("CAMPAIGN_ID".to_string(), vec!["summer2024".to_string()]);

    let originate_action2 = AmiAction::Originate {
        channel: "SIP/100".to_string(),
        application: None,
        data: None,
        timeout: None,
        caller_id: Some("1000".to_string()),
        context: Some("default".to_string()),
        exten: Some("200".to_string()),
        priority: Some(1),
        variables: Some(variables2),
        action_id: None,
    };

    println!("Sending second Originate action...");
    match manager.send_action(originate_action2).await {
        Ok(response) => {
            println!("Response: {:?}", response);
            if response.response.eq_ignore_ascii_case("Success") {
                println!("Second Originate action was successful!");
            }
        }
        Err(e) => {
            eprintln!("Error sending action: {}", e);
        }
    }

    // Example 3: Using Custom action for flexibility (supports duplicate keys)
    // This is useful for actions not explicitly defined or for advanced use cases
    println!("\n--- Example 3: Originate using Custom action (flexible variant) ---");
    
    let originate_custom = AmiAction::Custom {
        action: "Originate".to_string(),
        params: vec![
            ("Channel".to_string(), "PJSIP/user2".to_string()),
            ("Application".to_string(), "Playback".to_string()),
            ("Data".to_string(), "hello-world".to_string()),
            // Multiple Variable parameters with same key - only possible with Vec
            ("Variable".to_string(), "VAR1=value1".to_string()),
            ("Variable".to_string(), "VAR2=value2".to_string()),
            ("Variable".to_string(), "VAR3=value3".to_string()),
        ],
        action_id: None,
    };

    // Custom actions are also validated
    println!("Sending Custom Originate action...");
    match manager.send_action(originate_custom).await {
        Ok(response) => {
            println!("Response: {:?}", response);
        }
        Err(e) => {
            eprintln!("Error sending action: {}", e);
        }
    }

    // Example 4: Validation - this will fail because channel is empty
    println!("\n--- Example 4: Validation example (will fail) ---");
    
    let invalid_action = AmiAction::Originate {
        channel: "".to_string(), // Empty channel - validation will fail
        application: Some("Dial".to_string()),
        data: None,
        timeout: None,
        caller_id: None,
        context: None,
        exten: None,
        priority: None,
        variables: None,
        action_id: None,
    };

    println!("Sending invalid Originate action (should fail validation)...");
    match manager.send_action(invalid_action).await {
        Ok(_) => {
            println!("Unexpectedly succeeded!");
        }
        Err(e) => {
            println!("Expected validation error: {}", e);
        }
    }

    manager.disconnect().await?;
    println!("\nDisconnected from AMI.");
    Ok(())
}
