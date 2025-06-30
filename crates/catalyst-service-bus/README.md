# Catalyst Service Bus

Web2 integration layer for Catalyst Network that bridges blockchain events to traditional applications through WebSocket connections and REST APIs. Designed to make blockchain accessible to developers without blockchain expertise.

## Features

- **Real-time Event Streaming**: WebSocket-based event streaming with filtering
- **REST API**: Traditional HTTP endpoints for blockchain data
- **Event Filtering**: Sophisticated filtering by event type, addresses, contracts, and topics
- **Event Replay**: Access to historical events for catch-up scenarios
- **Authentication**: JWT and API key authentication with rate limiting
- **Multi-format Support**: JSON events compatible with any programming language
- **Automatic Reconnection**: Robust client with exponential backoff
- **Rate Limiting**: Configurable rate limits per connection
- **CORS Support**: Cross-origin resource sharing for web applications

## Quick Start

### Server Setup

```rust
use catalyst_service_bus::{ServiceBusConfig, ServiceBusServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServiceBusConfig::default();
    let server = ServiceBusServer::new(config).await?;
    server.start().await?;
    Ok(())
}
```

### Client Usage (Rust)

```rust
use catalyst_service_bus::{
    client::{ServiceBusClient, ClientConfig},
    events::{EventFilter, EventType},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ClientConfig {
        url: "ws://localhost:8080/ws".to_string(),
        ..Default::default()
    };
    
    let client = ServiceBusClient::new(config);
    client.connect().await?;
    
    // Subscribe to token transfers
    let filter = EventFilter::new()
        .with_event_types(vec![EventType::TokenTransfer]);
    
    client.subscribe(filter, |event| {
        println!("Token transfer: {:?}", event.data);
    }).await?;
    
    // Keep listening
    std::future::pending::<()>().await;
    Ok(())
}
```

### Client Usage (JavaScript/Node.js)

```javascript
const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:8080/ws');

ws.on('open', () => {
    console.log('Connected to Catalyst Service Bus');
    
    // Subscribe to transaction events
    const subscription = {
        type: 'Subscribe',
        filter: {
            event_types: ['TransactionConfirmed', 'TokenTransfer']
        },
        replay: false
    };
    
    ws.send(JSON.stringify(subscription));
});

ws.on('message', (data) => {
    const message = JSON.parse(data);
    
    if (message.type === 'Event') {
        console.log('Received event:', message.event);
        
        // Handle different event types
        switch (message.event.event_type) {
            case 'TokenTransfer':
                console.log('Token transfer:', message.event.data);
                break;
            case 'TransactionConfirmed':
                console.log('Transaction confirmed:', message.event.data);
                break;
        }
    }
});
```

### Client Usage (Python)

```python
import asyncio
import websockets
import json

async def listen_to_events():
    uri = "ws://localhost:8080/ws"
    
    async with websockets.connect(uri) as websocket:
        print("Connected to Catalyst Service Bus")
        
        # Subscribe to token transfers
        subscription = {
            "type": "Subscribe",
            "filter": {
                "event_types": ["TokenTransfer"]
            },
            "replay": False
        }
        
        await websocket.send(json.dumps(subscription))
        
        async for message in websocket:
            data = json.loads(message)
            
            if data["type"] == "Event":
                event = data["event"]
                print(f"Token transfer: {event['data']}")

# Run the client
asyncio.run(listen_to_events())
```

## Event Types

The Service Bus supports the following blockchain event types:

- `BlockFinalized` - New block added to chain
- `TransactionConfirmed` - Transaction included in block
- `TransactionPending` - Transaction added to mempool
- `ContractEvent` - Smart contract event
- `TokenTransfer` - Token transfer between addresses
- `BalanceUpdate` - Account balance change
- `ConsensusPhase` - Consensus phase transition
- `PeerConnected` - New peer connected
- `PeerDisconnected` - Peer disconnected
- `NetworkStats` - Network statistics update
- `Custom(String)` - Custom application events

## Event Filtering

Events can be filtered by multiple criteria:

```rust
let filter = EventFilter::new()
    .with_event_types(vec![EventType::TokenTransfer])
    .with_addresses(vec![user_address])
    .with_contracts(vec![token_contract_address])
    .with_topics(vec!["Transfer".to_string()])
    .with_block_range(Some(1000), Some(2000))
    .with_limit(100);
```

## REST API Endpoints

### Health Check
```
GET /api/v1/health
```

### Get Events
```
GET /api/v1/events?event_types=TokenTransfer&limit=100
```

### Get Event History
```
GET /api/v1/events/history?from_block=1000&to_block=2000&limit=50&offset=0
```

### Create Subscription
```
POST /api/v1/subscriptions
Content-Type: application/json

{
    "filter": {
        "event_types": ["TokenTransfer"]
    },
    "connection_id": "my_app_connection"
}
```

### Delete Subscription
```
DELETE /api/v1/subscriptions/{subscription_id}?connection_id=my_app_connection
```

## Configuration

The Service Bus can be configured via TOML file:

```toml
[service_bus]
host = "0.0.0.0"
port = 8080
max_connections = 1000

[service_bus.websocket]
enabled = true
path = "/ws"
ping_interval = "30s"
max_message_size = 1048576

[service_bus.rest_api]
enabled = true
path_prefix = "/api/v1"
max_body_size = 1048576

[service_bus.auth]
enabled = false
jwt_secret = "your_secret_here"
allow_anonymous = true

[service_bus.rate_limit]
enabled = true
requests_per_second = 100
burst_size = 10

[service_bus.event_filter]
max_filters_per_connection = 50
enable_replay = true
max_replay_duration = "1h"
replay_buffer_size = 10000

[service_bus.cors]
enabled = true
allowed_origins = ["*"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
allow_credentials = false
```

## Authentication

### JWT Authentication

```rust
use catalyst_service_bus::auth::AuthManager;

let auth_manager = AuthManager::new(auth_config)?;

let token = auth_manager.generate_token(
    "user_id".to_string(),
    vec!["read".to_string(), "write".to_string()],
    Some(5), // max connections
    None,    // no rate limit override
)?;
```

Use the token in WebSocket connection:
```
ws://localhost:8080/ws?token=YOUR_JWT_TOKEN
```

Or in HTTP headers:
```
Authorization: Bearer YOUR_JWT_TOKEN
```

### API Key Authentication

```
X-API-Key: YOUR_API_KEY
```

## WebSocket Message Format

### Subscribe to Events
```json
{
    "type": "Subscribe",
    "filter": {
        "event_types": ["TokenTransfer"],
        "addresses": ["0x123..."],
        "from_block": 1000,
        "limit": 100
    },
    "replay": true
}
```

### Event Message
```json
{
    "type": "Event",
    "subscription_id": "uuid",
    "event": {
        "id": "uuid",
        "event_type": "TokenTransfer",
        "block_number": 12345,
        "transaction_hash": "0x...",
        "data": {
            "from": "0x...",
            "to": "0x...",
            "amount": 1000,
            "symbol": "KAT"
        },
        "timestamp": 1640995200,
        "topics": ["Transfer"]
    }
}
```

### Unsubscribe
```json
{
    "type": "Unsubscribe",
    "subscription_id": "uuid"
}
```

### Get Historical Events
```json
{
    "type": "GetHistory",
    "filter": {
        "event_types": ["TransactionConfirmed"],
        "from_block": 1000
    },
    "limit": 50
}
```

## Error Handling

All errors are returned in a consistent format:

```json
{
    "type": "Error",
    "message": "Error description",
    "code": "ERROR_CODE"
}
```

Common error codes:
- `INVALID_MESSAGE` - Malformed WebSocket message
- `AUTH_FAILED` - Authentication failed
- `RATE_LIMIT` - Rate limit exceeded
- `INVALID_FILTER` - Invalid event filter
- `SUBSCRIPTION_NOT_FOUND` - Subscription doesn't exist
- `MAX_CONNECTIONS` - Connection limit reached

## Examples

See the `examples/` directory for complete examples:

- `basic_client.rs` - Basic Rust client usage
- `web_integration.js` - JavaScript/Node.js integration
- `python_client.py` - Python client example
- `e_commerce.rs` - E-commerce payment integration
- `monitoring.rs` - Network monitoring dashboard

## Integration Patterns

### E-commerce Payment Processing

```rust
// Listen for payment confirmations
let payment_filter = EventFilter::new()
    .with_event_types(vec![EventType::TransactionConfirmed])
    .with_addresses(vec![payment_processor_address]);

client.subscribe(payment_filter, |event| {
    if let Some(order_id) = event.metadata.custom.get("order_id") {
        // Update order status in database
        update_order_status(order_id, "paid").await;
        send_confirmation_email(order_id).await;
    }
}).await?;
```

### DeFi Portfolio Tracking

```rust
// Track token transfers for a portfolio
let portfolio_addresses = vec![user_wallet, yield_farm_contract];

let portfolio_filter = EventFilter::new()
    .with_event_types(vec![EventType::TokenTransfer, EventType::BalanceUpdate])
    .with_addresses(portfolio_addresses);

client.subscribe(portfolio_filter, |event| {
    update_portfolio_metrics(event).await;
    if significant_change(&event) {
        notify_user(&event).await;
    }
}).await?;
```

### Real-time Analytics

```rust
// Collect network statistics
let analytics_filter = EventFilter::new()
    .with_event_types(vec![
        EventType::BlockFinalized,
        EventType::TransactionConfirmed,
        EventType::NetworkStats,
    ]);

client.subscribe(analytics_filter, |event| {
    match event.event_type {
        EventType::BlockFinalized => {
            update_block_metrics(event.block_number).await;
        }
        EventType::TransactionConfirmed => {
            update_transaction_metrics(&event).await;
        }
        EventType::NetworkStats => {
            update_network_dashboard(&event).await;
        }
        _ => {}
    }
}).await?;
```

## Development

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
```

### Running Examples

```bash
# Start the service bus server
cargo run --example server

# In another terminal, run a client example
cargo run --example basic_client
```

### Docker

```dockerfile
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/catalyst-service-bus /usr/local/bin/
EXPOSE 8080
CMD ["catalyst-service-bus"]
```

## Performance

The Service Bus is designed for high throughput:

- **WebSocket Connections**: 10,000+ concurrent connections
- **Event Throughput**: 100,000+ events/second
- **Latency**: <1ms event processing
- **Memory Usage**: ~50MB base + ~1KB per active subscription
- **Replay Buffer**: Configurable, 10,000 events by default

## Security

- JWT-based authentication with configurable expiration
- API key authentication for service-to-service communication
- Rate limiting per connection with burst support
- CORS configuration for web applications
- Input validation and sanitization
- Connection limits and resource protection

## Monitoring

Built-in metrics and logging:

- Connection counts and subscription statistics
- Event processing rates and latencies
- Error rates and types
- Resource usage (memory, CPU)
- Authentication failures and rate limit hits

Integration with monitoring systems:
- Prometheus metrics endpoint
- Structured JSON logging
- Health check endpoints
- Custom metrics via callback hooks

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run the test suite
6. Submit a pull request

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.