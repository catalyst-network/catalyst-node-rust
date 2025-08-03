# Catalyst RPC

JSON-RPC server implementation for the Catalyst blockchain network. This crate provides a comprehensive HTTP-based API for interacting with Catalyst nodes, compatible with standard blockchain RPC interfaces.

## Features

- **JSON-RPC 2.0**: Standards-compliant JSON-RPC server
- **Comprehensive API**: Full blockchain interaction capabilities
- **Ethereum Compatibility**: Similar method names and structures for easy migration
- **Real-time Data**: Live blockchain state and transaction information
- **Developer Friendly**: Easy integration with web applications and tools

## Installation

Add to your `Cargo.toml`:
```toml
[dependencies]
catalyst-rpc = { path = "../catalyst-rpc" }
tokio = { version = "1.0", features = ["full"] }
```

## Quick Start

### Basic Server Setup
```rust
use catalyst_rpc::{RpcServer, RpcConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = RpcConfig {
        enabled: true,
        port: 9933,
        address: "127.0.0.1".to_string(),
        max_connections: 100,
        cors_enabled: true,
        cors_origins: vec!["*".to_string()],
    };
    
    let server = RpcServer::new(config);
    server.start().await?;
    
    println!("RPC server running on http://127.0.0.1:9933");
    
    // Keep server running
    tokio::signal::ctrl_c().await?;
    server.stop().await?;
    
    Ok(())
}
```

### Testing the Server
```bash
# Start the server
cargo run

# Test in another terminal
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_version","params":[],"id":1}'
```

## API Reference

### Node Information

#### `catalyst_version`
Get the node version.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_version","params":[],"id":1}'
```
Response:
```json
{"jsonrpc":"2.0","result":"Catalyst/1.0.0","id":1}
```

#### `catalyst_status`
Get comprehensive node status.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_status","params":[],"id":1}'
```
Response:
```json
{
  "jsonrpc":"2.0",
  "result":{
    "node_id":"catalyst-node",
    "uptime":3600,
    "sync_status":"synced",
    "block_height":12345,
    "peer_count":8,
    "cpu_usage":25.5,
    "memory_usage":536870912,
    "disk_usage":2147483648
  },
  "id":1
}
```

#### `catalyst_peerCount`
Get the number of connected peers.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_peerCount","params":[],"id":1}'
```

#### `catalyst_networkInfo`
Get detailed network information.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_networkInfo","params":[],"id":1}'
```

#### `catalyst_syncing`
Get synchronization status.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_syncing","params":[],"id":1}'
```

### Blockchain State

#### `catalyst_blockNumber`
Get the current block number.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_blockNumber","params":[],"id":1}'
```

#### `catalyst_getLatestBlock`
Get the latest block information.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getLatestBlock","params":[false],"id":1}'
```

#### `catalyst_getBlockByHash`
Get block by hash.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getBlockByHash","params":["0x1234...","false"],"id":1}'
```

#### `catalyst_getBlockByNumber`
Get block by number.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getBlockByNumber","params":["latest","false"],"id":1}'
```

### Transactions

#### `catalyst_getTransactionByHash`
Get transaction details by hash.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getTransactionByHash","params":["0xabc123..."],"id":1}'
```

#### `catalyst_getTransactionReceipt`
Get transaction receipt.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getTransactionReceipt","params":["0xabc123..."],"id":1}'
```

#### `catalyst_sendRawTransaction`
Send a raw transaction.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_sendRawTransaction","params":["0x..."],"id":1}'
```

#### `catalyst_estimateFee`
Estimate transaction fee.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc":"2.0",
    "method":"catalyst_estimateFee",
    "params":[{
      "from":"0x123...",
      "to":"0x456...",
      "value":"1000000000000000000"
    }],
    "id":1
  }'
```

#### `catalyst_pendingTransactions`
Get pending transactions.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_pendingTransactions","params":[],"id":1}'
```

### Accounts & Balances

#### `catalyst_getBalance`
Get account balance.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getBalance","params":["0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e"],"id":1}'
```

#### `catalyst_getAccount`
Get account information.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getAccount","params":["0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e"],"id":1}'
```

#### `catalyst_getTransactionCount`
Get account nonce.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getTransactionCount","params":["0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e"],"id":1}'
```

### Smart Contracts

#### `catalyst_call`
Call contract method (read-only).
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc":"2.0",
    "method":"catalyst_call",
    "params":[{
      "to":"0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e",
      "data":"0x70a08231000000000000000000000000742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e"
    }],
    "id":1
  }'
```

#### `catalyst_estimateGas`
Estimate gas for transaction.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc":"2.0",
    "method":"catalyst_estimateGas",
    "params":[{
      "to":"0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e",
      "data":"0xa9059cbb000000000000000000000000..."
    }],
    "id":1
  }'
```

#### `catalyst_getCode`
Get contract code.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getCode","params":["0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e"],"id":1}'
```

#### `catalyst_getStorageAt`
Get storage value at position.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getStorageAt","params":["0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e","0x0"],"id":1}'
```

### Validation & Mining

#### `catalyst_validatorStatus`
Get validator status.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_validatorStatus","params":[],"id":1}'
```

#### `catalyst_getValidators`
Get list of validators.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getValidators","params":[],"id":1}'
```

#### `catalyst_submitBlock`
Submit a new block (validators only).
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_submitBlock","params":["0x..."],"id":1}'
```

### Chain Information

#### `catalyst_chainId`
Get chain ID.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_chainId","params":[],"id":1}'
```

#### `catalyst_gasPrice`
Get current gas price.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_gasPrice","params":[],"id":1}'
```

#### `catalyst_getGenesis`
Get genesis block.
```bash
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_getGenesis","params":[],"id":1}'
```

## Configuration

### RpcConfig Structure
```rust
pub struct RpcConfig {
    pub enabled: bool,           // Enable/disable RPC server
    pub port: u16,              // Server port (default: 9933)
    pub address: String,        // Bind address (default: "127.0.0.1")
    pub max_connections: u32,   // Max concurrent connections
    pub cors_enabled: bool,     // Enable CORS
    pub cors_origins: Vec<String>, // CORS allowed origins
}
```

### Example Configurations

#### Development Server
```rust
let config = RpcConfig {
    enabled: true,
    port: 9933,
    address: "127.0.0.1".to_string(),
    max_connections: 100,
    cors_enabled: true,
    cors_origins: vec!["*".to_string()],
};
```

#### Production Server
```rust
let config = RpcConfig {
    enabled: true,
    port: 9933,
    address: "0.0.0.0".to_string(),
    max_connections: 1000,
    cors_enabled: true,
    cors_origins: vec![
        "https://myapp.com".to_string(),
        "https://wallet.myapp.com".to_string(),
    ],
};
```

## Integration Examples

### Web3 JavaScript
```javascript
// Using standard fetch
async function getBlockNumber() {
  const response = await fetch('http://127.0.0.1:9933', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      method: 'catalyst_blockNumber',
      params: [],
      id: 1
    })
  });
  
  const result = await response.json();
  return result.result;
}

// Using Web3.js-style provider
class CatalystProvider {
  constructor(url) {
    this.url = url;
    this.id = 1;
  }
  
  async send(method, params = []) {
    const response = await fetch(this.url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        method,
        params,
        id: this.id++
      })
    });
    
    const result = await response.json();
    if (result.error) throw new Error(result.error.message);
    return result.result;
  }
}

const provider = new CatalystProvider('http://127.0.0.1:9933');
const balance = await provider.send('catalyst_getBalance', ['0x123...']);
```

### Python Client
```python
import requests
import json

class CatalystClient:
    def __init__(self, url='http://127.0.0.1:9933'):
        self.url = url
        self.id = 1
    
    def call(self, method, params=[]):
        payload = {
            'jsonrpc': '2.0',
            'method': method,
            'params': params,
            'id': self.id
        }
        self.id += 1
        
        response = requests.post(
            self.url,
            headers={'Content-Type': 'application/json'},
            data=json.dumps(payload)
        )
        
        result = response.json()
        if 'error' in result:
            raise Exception(result['error']['message'])
        
        return result['result']

# Usage
client = CatalystClient()
block_number = client.call('catalyst_blockNumber')
balance = client.call('catalyst_getBalance', ['0x123...'])
```

### Rust Client
```rust
use reqwest::Client;
use serde_json::{json, Value};

pub struct CatalystClient {
    client: Client,
    url: String,
    id: u64,
}

impl CatalystClient {
    pub fn new(url: String) -> Self {
        Self {
            client: Client::new(),
            url,
            id: 1,
        }
    }
    
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, Box<dyn std::error::Error>> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": self.id
        });
        
        self.id += 1;
        
        let response = self.client
            .post(&self.url)
            .json(&payload)
            .send()
            .await?;
        
        let result: Value = response.json().await?;
        
        if let Some(error) = result.get("error") {
            return Err(error["message"].as_str().unwrap_or("Unknown error").into());
        }
        
        Ok(result["result"].clone())
    }
}

// Usage
let mut client = CatalystClient::new("http://127.0.0.1:9933".to_string());
let block_number = client.call("catalyst_blockNumber", json!([])).await?;
```

## Error Handling

### Standard JSON-RPC Errors
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32600,
    "message": "Invalid Request"
  },
  "id": null
}
```

### Common Error Codes
- `-32700`: Parse error
- `-32600`: Invalid Request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32603`: Internal error

### Catalyst-Specific Errors
- `-32000`: Server error
- `-32001`: Transaction not found
- `-32002`: Block not found
- `-32003`: Account not found

## Performance & Security

### Rate Limiting
```rust
let config = RpcConfig {
    max_connections: 100,  // Limit concurrent connections
    // ... other config
};
```

### CORS Configuration
```rust
let config = RpcConfig {
    cors_enabled: true,
    cors_origins: vec![
        "https://trusted-domain.com".to_string(),
        "https://wallet.trusted-domain.com".to_string(),
    ],
    // ... other config
};
```

### Monitoring
```bash
# Check server status
curl -s http://127.0.0.1:9933 \
  -d '{"jsonrpc":"2.0","method":"catalyst_status","params":[],"id":1}' \
  | jq '.result'

# Monitor connections
netstat -an | grep :9933
```

## Testing

### Unit Tests
```bash
cargo test
```

### Integration Tests
```bash
# Start test server
cargo run --example test_server

# Run integration tests
cargo test --test integration
```

### Load Testing
```bash
# Using Apache Bench
ab -n 1000 -c 10 -T application/json -p request.json http://127.0.0.1:9933/

# request.json content:
# {"jsonrpc":"2.0","method":"catalyst_blockNumber","params":[],"id":1}
```

## Contributing

1. Implement new RPC methods in the `CatalystRpc` trait
2. Add corresponding types in `types.rs`
3. Implement the method in `CatalystRpcImpl`
4. Add tests and documentation
5. Update this README

## License

This crate is part of the Catalyst Network project and is licensed under [LICENSE](../../LICENSE).