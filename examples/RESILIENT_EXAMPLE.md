# Complete Resilient Asterisk AMI Connection Example

This example demonstrates how to utilize the full potential of the resilient connection features in the `rust-asterisk-manager` library.

## 🚀 Implemented Features

### 1. **Resilient Connection with Infinite Retry**
- Automatic reconnection without retry limit
- Fixed 5-second delay between attempts
- Detailed logging of each reconnection attempt

### 2. **Comprehensive Metrics**
- **Reconnection Metrics**: attempts, successes, failures
- **Connection Metrics**: loss events, last reconnection duration
- **Application Metrics**: uptime, buffer utilization, current status

### 3. **Status Monitoring**
- Real-time uptime/downtime tracking
- Health check endpoint for external monitoring
- Real-time connection status updates

### 4. **Advanced Configuration**
- Increased buffer (4096) for high-traffic scenarios
- Configurable heartbeat via environment variable
- Global cumulative attempt counter
- Automatic watchdog for failure detection

## 📡 API Endpoints

### `GET /events`
Returns recent AMI events stored in the buffer.

### `POST /action`
Sends a custom AMI action.
```json
{
  "action": "Status",
  "params": {
    "Channel": "SIP/1000-0001"
  }
}
```

### `GET /calls`
Lists active calls using the `CoreShowChannels` action.

### `GET /metrics` ⭐ **NEW**
Returns detailed resilient connection metrics:
```json
{
  "resilient_metrics": {
    "reconnection_attempts": 15,
    "successful_reconnections": 14,
    "failed_reconnections": 1,
    "connection_lost_events": 3,
    "last_reconnection_duration_ms": 150
  },
  "connection_status": {
    "is_connected": true,
    "last_connection_attempt": 30,
    "last_successful_connection": 25,
    "total_uptime_seconds": 3600,
    "total_downtime_seconds": 45
  },
  "application_metrics": {
    "total_uptime_seconds": 3645,
    "events_in_buffer": 234,
    "buffer_utilization_percent": 23.4
  }
}
```

### `GET /health` ⭐ **NEW**
Health check endpoint for monitoring:
```json
{
  "status": "healthy", // or "degraded"
  "is_connected": true,
  "reconnection_attempts": 15,
  "connection_lost_events": 3
}
```

## 🔧 Configuration via Environment Variables

```bash
# Connection configuration
export AMI_HOST=192.168.1.100
export AMI_PORT=5038
export AMI_USERNAME=admin
export AMI_PASSWORD=secret

# Monitoring configuration
export AMI_HEARTBEAT_INTERVAL=60  # seconds

# Logging configuration
export RUST_LOG=debug  # For detailed reconnection logs
```

## 🏃 How to Run

```bash
# 1. Configure environment variables
export AMI_HOST=your_asterisk_ip
export AMI_USERNAME=your_username
export AMI_PASSWORD=your_password

# 2. Run with detailed logs
RUST_LOG=debug cargo run --example actix_web_resilient_example

# 3. Test endpoints
curl http://localhost:8080/health
curl http://localhost:8080/metrics
curl http://localhost:8080/events
```

## 📊 Production Monitoring

### Metrics Dashboard
```bash
# Check connection status
curl -s http://localhost:8080/health | jq '.status'

# Monitor reconnection metrics
curl -s http://localhost:8080/metrics | jq '.resilient_metrics'

# Check buffer utilization
curl -s http://localhost:8080/metrics | jq '.application_metrics.buffer_utilization_percent'
```

### Recommended Alerts
- **status != "healthy"**: Connectivity issues
- **connection_lost_events > 10**: Network instability
- **buffer_utilization_percent > 80%**: Buffer near limit
- **total_downtime_seconds increasing**: Persistent issues

## 🛠 Technical Characteristics

### Resilient Stream
- **Infinite Loop**: Never stops trying to reconnect
- **Fixed Delay**: 5 seconds between attempts (no exponential backoff)
- **Global Counter**: Tracks cumulative attempts across instances
- **Stream ID**: Unique identification for logs `[uuid]`

### Buffer Management
- **Size**: 4096 events (configurable)
- **Rotation**: Automatically removes old events
- **Metrics**: Real-time utilization monitoring

### Connection Monitoring
- **Heartbeat**: Configurable periodic ping
- **Watchdog**: Automatic failure detection
- **Status Tracking**: Precision uptime/downtime

## 🧪 Testing Resilience

```bash
# 1. Start the example
RUST_LOG=debug cargo run --example actix_web_resilient_example

# 2. In another terminal, simulate network failure
sudo iptables -A OUTPUT -p tcp --dport 5038 -j DROP

# 3. Check continuous reconnection logs
# Logs will show attempts every 5 seconds

# 4. Restore connectivity
sudo iptables -D OUTPUT -p tcp --dport 5038 -j DROP

# 5. Check metrics
curl http://localhost:8080/metrics
```

## 📈 Use Cases

1. **Web Applications**: Web interface for AMI with automatic resilience
2. **Monitoring**: Telephony dashboards with connectivity metrics  
3. **Integrations**: APIs requiring stable AMI connection
4. **Production**: Critical systems with detailed monitoring

This example demonstrates a complete and production-ready implementation of a resilient connection with Asterisk AMI, including all advanced library features.
