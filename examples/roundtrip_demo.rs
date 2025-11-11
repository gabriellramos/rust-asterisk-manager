//! Demonstration that the serialization/deserialization bug is fixed
//!
//! This example demonstrates that AmiEvent::UnknownEvent can now be:
//! 1. Serialized to JSON
//! 2. Sent through a message queue (simulated)
//! 3. Deserialized back without data loss
//!
//! This was previously broken - the event_type would become "UnknownOrMalformed"

use asterisk_manager::AmiEvent;
use std::collections::HashMap;

fn main() {
    println!("=== AmiEvent::UnknownEvent Serialization Round-Trip Test ===\n");

    // 1. Create an UnknownEvent (as the library does when receiving from Asterisk)
    let mut fields = HashMap::new();
    fields.insert("Event".to_string(), "ContactStatus".to_string());
    fields.insert("AOR".to_string(), "1000021005".to_string());
    fields.insert("ContactStatus".to_string(), "Removed".to_string());
    fields.insert("URI".to_string(), "sip:1000021005@10.0.0.1:5060".to_string());

    let original = AmiEvent::UnknownEvent {
        event_type: "ContactStatus".to_string(),
        fields: fields.clone(),
    };

    println!("1. Original event:");
    println!("   {:?}\n", original);

    // 2. Serialize to JSON (e.g., to send via Kafka, RabbitMQ, etc.)
    let json = serde_json::to_string(&original).unwrap();
    println!("2. Serialized to JSON:");
    println!("   {}\n", json);

    // 3. Deserialize back (e.g., consumer receives from message queue)
    let deserialized: AmiEvent = serde_json::from_str(&json).unwrap();
    println!("3. Deserialized event:");
    println!("   {:?}\n", deserialized);

    // 4. Verify the data is preserved
    match deserialized {
        AmiEvent::UnknownEvent { event_type, fields } => {
            println!("✅ SUCCESS: Event round-trip completed successfully!");
            println!("   - Event type: {} (preserved)", event_type);
            println!("   - AOR: {:?} (preserved)", fields.get("AOR"));
            println!("   - ContactStatus: {:?} (preserved)", fields.get("ContactStatus"));
            println!("   - URI: {:?} (preserved)", fields.get("URI"));
            println!("\n✅ All fields preserved correctly!");
            
            // Verify assertions
            assert_eq!(event_type, "ContactStatus");
            assert_eq!(fields.get("AOR"), Some(&"1000021005".to_string()));
            assert_eq!(fields.get("ContactStatus"), Some(&"Removed".to_string()));
        }
        other => {
            println!("❌ FAILURE: Got unexpected event type: {:?}", other);
            panic!("Round-trip failed!");
        }
    }

    println!("\n=== Test Complete ===");
}
