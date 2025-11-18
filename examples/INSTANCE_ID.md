# Instance ID for Logging

## What It Is

Each `Manager` instance now has a unique **instance_id** (first 8 characters of a UUID v4), automatically generated at creation time. This ID appears in all logs associated with that specific instance.

## Why It Matters

When you maintain multiple simultaneous AMI connections (e.g. different Asterisk servers or several connections to the same server), the `instance_id` lets you immediately distinguish which `Manager` produced a log line.

## Where It Appears in Logs

The `instance_id` is included in logs for:

### Core Manager
- Creation: `Creating new Manager instance [abc12345]`

### Heartbeat
- Start: `[abc12345] Starting heartbeat task (interval=30s)`
- Success tick: `[abc12345] Heartbeat ping successful`
- Failure: `[abc12345] Heartbeat ping failed: <error>`
- Cancellation: `[abc12345] Heartbeat task cancelled`

### Watchdog
- Start: `[abc12345] Starting watchdog (default interval=1s) for user 'admin' at 127.0.0.1:5038`
- Attempt: `[abc12345] Watchdog attempting reconnection to 'admin'@127.0.0.1:5038...`
- Success: `[abc12345] Watchdog reconnection successful to 'admin'@127.0.0.1:5038`
- Failure: `[abc12345] Watchdog reconnection to 'admin'@127.0.0.1:5038 failed: <error>`
- Cancellation: `[abc12345] Watchdog task cancelled by token`

### Resilient Module
- Connection: `[abc12345] Connecting resilient AMI manager to 'admin'@127.0.0.1:5038`
- Stream creation: `Creating new resilient stream instance [def67890]` (streams have their own IDs)

## Practical Examples

### Single Connection Logs
```
[2025-10-20 ... DEBUG asterisk_manager] Creating new Manager instance [a1b2c3d4]
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Starting heartbeat task (interval=30s)
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Heartbeat ping successful
```

### Multiple Concurrent Connections
```
[2025-10-20 ... DEBUG asterisk_manager] Creating new Manager instance [a1b2c3d4]
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Connecting resilient AMI manager to 'admin'@127.0.0.1:5038
[2025-10-20 ... DEBUG asterisk_manager] Creating new Manager instance [e5f6g7h8]
[2025-10-20 ... DEBUG asterisk_manager] [e5f6g7h8] Connecting resilient AMI manager to 'admin2'@127.0.0.1:5039
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Heartbeat ping successful
[2025-10-20 ... DEBUG asterisk_manager] [e5f6g7h8] Watchdog attempting reconnection to 'admin2'@127.0.0.1:5039...
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Watchdog tick: already authenticated; no action taken
```

In this example you can clearly see:
- `[a1b2c3d4]` is the first connection (port 5038)
- `[e5f6g7h8]` is the second connection (port 5039)

## When to Use It in Your Own Logs

The `instance_id` is especially useful for:

1. **Debugging multiple connections**: Quickly isolate which connection is misbehaving
2. **Monitoring**: Track per-connection metrics or performance
3. **Troubleshooting**: Follow the full lifecycle of a specific connection
4. **Auditing**: Correlate events over time for one logical instance

## Included Examples

- `watchdog_resilient_logging.rs`: Shows `instance_id` in a single connection scenario
- `multiple_managers_logging.rs`: Demonstrates `instance_id` across multiple concurrent connections

## Enabling Detailed Logs

To view all logs with `instance_id`:

```bash
# Debug-level logs for all operations
RUST_LOG=asterisk_manager=debug cargo run --example multiple_managers_logging

# Trace-level logs including heartbeat/watchdog ticks
RUST_LOG=asterisk_manager=trace cargo run --example multiple_managers_logging
```
