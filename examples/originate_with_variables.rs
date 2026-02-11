/// Example demonstrating how to use the Originate action with multiple Variables and optional parameters.
///
/// This example shows how to send an Originate action with multiple Variable parameters
/// and all the optional fields supported by the Asterisk AMI Originate command.
/// It demonstrates both the new typed AmiAction::Originate variant and the
/// flexible AmiAction::Custom variant with Vec<(String, String)> for params.
///
/// The new Originate variant provides:
/// - Type safety with explicit fields
/// - HashMap<String, Vec<String>> for variables supporting:
///   - Multiple variables with individual values
///   - Each variable can have multiple values
/// - All optional parameters from the official AMI documentation:
///   - Account: for billing/tracking in CDRs
///   - EarlyMedia: force bridge on early media
///   - Async: originate asynchronously
///   - Codecs: specify codec list
///   - ChannelId: custom channel unique ID
///   - OtherChannelId: custom ID for second channel (Local channels)
///   - PreDialGoSub: execute GoSub before dialing
/// - Automatic validation of required fields
/// - Automatic validation of keys and values
///
/// For full documentation, see:
/// https://docs.asterisk.org/Latest_API/API_Documentation/AMI_Actions/Originate/
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
    // Each variable can have multiple values in the data structure.
    // Note: The AMI protocol will send each value as a separate Variable line.
    // This is mainly useful for programmatically building variable lists.
    variables.insert("CDR(extra_data)".to_string(), vec!["123".to_string()]);
    variables.insert("__ID_EXTRA".to_string(), vec!["456".to_string()]);
    variables.insert("__ID_MAIN".to_string(), vec!["789".to_string()]);

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
        account: None,
        early_media: None,
        async_originate: None,
        codecs: None,
        channel_id: None,
        other_channel_id: None,
        pre_dial_go_sub: None,
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
        account: None,
        early_media: None,
        async_originate: None,
        codecs: None,
        channel_id: None,
        other_channel_id: None,
        pre_dial_go_sub: None,
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
        account: None,
        early_media: None,
        async_originate: None,
        codecs: None,
        channel_id: None,
        other_channel_id: None,
        pre_dial_go_sub: None,
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

    // Example 5: Using all new optional parameters
    println!("\n--- Example 5: Originate with all optional parameters ---");
    println!("Demonstrates: Account, EarlyMedia, Async, Codecs, ChannelId, OtherChannelId, and PreDialGoSub");
    
    let advanced_originate = AmiAction::Originate {
        channel: "PJSIP/user3".to_string(),
        application: Some("Playback".to_string()),
        data: Some("demo-congrats".to_string()),
        timeout: Some(45000),
        caller_id: Some("2000".to_string()),
        context: None,
        exten: None,
        priority: None,
        variables: None,
        // New optional fields demonstrating various use cases:
        account: Some("project_alpha".to_string()), // For billing/tracking in CDRs
        early_media: Some(true), // Force bridge on early media (useful for IVR prompts before answer)
        async_originate: Some(true), // Originate call asynchronously (don't wait for answer)
        codecs: Some("ulaw,alaw,gsm".to_string()), // Restrict codecs for this call
        channel_id: Some("CustomID-12345".to_string()), // Custom channel unique ID for tracking
        other_channel_id: Some("CustomID-67890".to_string()), // For Local channel pairs
        pre_dial_go_sub: Some("predial_context,s,1".to_string()), // Execute GoSub before dialing
        action_id: Some("advanced123".to_string()),
    };

    println!("Sending Originate with all optional parameters...");
    match manager.send_action(advanced_originate).await {
        Ok(response) => {
            println!("Response: {:?}", response);
            if response.response.eq_ignore_ascii_case("Success") {
                println!("Advanced Originate action was successful!");
                println!("Note: The call was originated asynchronously.");
                println!("  - Account 'project_alpha' will appear in CDRs");
                println!("  - Early media is enabled for IVR prompts before answer");
                println!("  - Codecs restricted to: ulaw, alaw, gsm");
                println!("  - Custom channel IDs set for tracking");
                println!("  - PreDialGoSub executed before dialing");
            }
        }
        Err(e) => {
            eprintln!("Error sending action: {}", e);
        }
    }

    // Example 6: Using PreDialGoSub for adding custom SIP headers
    println!("\n--- Example 6: PreDialGoSub for custom SIP headers ---");
    
    let predial_originate = AmiAction::Originate {
        channel: "PJSIP/user4".to_string(),
        application: Some("Dial".to_string()),
        data: Some("PJSIP/destination@trunk".to_string()),
        timeout: None,
        caller_id: Some("3000".to_string()),
        context: None,
        exten: None,
        priority: None,
        variables: None,
        account: None,
        early_media: None,
        async_originate: Some(false), // Wait for the call to be answered
        codecs: None,
        channel_id: None,
        other_channel_id: None,
        // PreDialGoSub can be used to add custom SIP headers or set variables
        // The format is: Context,Extension,Priority
        // In your dialplan, create the corresponding context to add headers
        pre_dial_go_sub: Some("add_headers,s,1".to_string()),
        action_id: None,
    };

    println!("Sending Originate with PreDialGoSub for custom headers...");
    println!("Note: Make sure you have a 'add_headers' context in your dialplan");
    println!("Example dialplan:");
    println!("  [add_headers]");
    println!("  exten => s,1,Set(PJSIP_HEADER(add,X-Custom-Header)=CustomValue)");
    println!("  exten => s,n,Return()");
    
    match manager.send_action(predial_originate).await {
        Ok(response) => {
            println!("Response: {:?}", response);
        }
        Err(e) => {
            eprintln!("Error sending action: {}", e);
        }
    }

    // Example 7: Using Account and Codecs for billing and quality control
    println!("\n--- Example 7: Account for billing and Codecs for quality control ---");
    
    let billing_originate = AmiAction::Originate {
        channel: "PJSIP/premium_user".to_string(),
        application: Some("Dial".to_string()),
        data: Some("PJSIP/service@provider".to_string()),
        timeout: Some(60000),
        caller_id: Some("4000".to_string()),
        context: None,
        exten: None,
        priority: None,
        variables: None,
        account: Some("premium_service_Q1_2024".to_string()), // Detailed billing code
        early_media: None,
        async_originate: None,
        codecs: Some("g722,ulaw".to_string()), // High quality codecs only
        channel_id: None,
        other_channel_id: None,
        pre_dial_go_sub: None,
        action_id: None,
    };

    println!("Sending Originate for premium service...");
    println!("  - Account code 'premium_service_Q1_2024' will be in CDRs");
    println!("  - High quality codecs enforced: g722, ulaw");
    
    match manager.send_action(billing_originate).await {
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
