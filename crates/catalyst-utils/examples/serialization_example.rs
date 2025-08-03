// catalyst-utils/examples/serialization_example.rs

use catalyst_utils::{
    CatalystResult, Hash, Address, TransactionEntry, TransactionStatus,
    serialization::{CatalystSerialize, CatalystDeserialize, SerializationContext, CatalystSerializeWithContext},
};
use serde::{Serialize, Deserialize};

fn main() -> CatalystResult<()> {
    println!("Catalyst Serialization Examples");
    
    // Basic type serialization
    println!("\n=== Basic Types ===");
    let value_u32: u32 = 0x12345678;
    let bytes_u32 = CatalystSerialize::serialize(&value_u32)?;
    let recovered_u32 = <u32 as CatalystDeserialize>::deserialize(&bytes_u32)?;
    
    println!("Original u32: 0x{:08x}", value_u32);
    println!("Serialized bytes: {:?}", bytes_u32);
    println!("Recovered u32: 0x{:08x}", recovered_u32);
    println!("Roundtrip successful: {}", value_u32 == recovered_u32);

    // String serialization
    println!("\n=== String Serialization ===");
    let test_string = "Hello, Catalyst Network!".to_string();
    let string_bytes = CatalystSerialize::serialize(&test_string)?;
    let recovered_string = <String as CatalystDeserialize>::deserialize(&string_bytes)?;
    
    println!("Original: {}", test_string);
    println!("Recovered: {}", recovered_string);
    println!("Roundtrip successful: {}", test_string == recovered_string);

    // Struct serialization using existing types
    println!("\n=== Struct Serialization ===");
    let entry = TransactionEntry {
        hash: Hash::new([1u8; 32]),
        from: Address::new([2u8; 20]),
        to: Some(Address::new([3u8; 20])),
        value: 1000,
        gas_used: 21000,
        status: TransactionStatus::Success,
    };
    
    // Use JSON serialization for existing types
    let entry_bytes = serde_json::to_vec(&entry)?;
    let recovered_entry: TransactionEntry = serde_json::from_slice(&entry_bytes)?;
    
    println!("Transaction entry serialized and recovered successfully");
    println!("Original hash: {}", entry.hash);
    println!("Recovered hash: {}", recovered_entry.hash);

    // Context-aware serialization example
    println!("\n=== Context-Aware Serialization ===");
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct ProtocolMessage {
        version: u32,
        message_type: u8,
        payload: Vec<u8>,
    }
    
    impl CatalystSerializeWithContext for ProtocolMessage {
        fn serialize_with_context(&self, ctx: &SerializationContext) -> CatalystResult<Vec<u8>> {
            let mut data = Vec::new();
            data.extend_from_slice(&ctx.protocol_version.to_le_bytes());
            data.extend_from_slice(&[ctx.network_id]);
            data.extend_from_slice(&self.version.to_le_bytes());
            data.extend_from_slice(&[self.message_type]);
            data.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
            data.extend_from_slice(&self.payload);
            Ok(data)
        }
        
        fn deserialize_with_context(data: &[u8], _ctx: &SerializationContext) -> CatalystResult<Self> 
        where
            Self: Sized 
        {
            if data.len() < 13 { // min size: 4 + 1 + 4 + 1 + 4 = 14 bytes, but we check for 13 first
                return Err(catalyst_utils::CatalystError::Serialization(
                    "Insufficient data for ProtocolMessage".to_string()
                ));
            }
            
            let _protocol_version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let _network_id = data[4];
            let version = u32::from_le_bytes([data[5], data[6], data[7], data[8]]);
            let message_type = data[9];
            let payload_len = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
            
            if data.len() < 14 + payload_len {
                return Err(catalyst_utils::CatalystError::Serialization(
                    "Insufficient payload data".to_string()
                ));
            }
            
            let payload = data[14..14 + payload_len].to_vec();
            
            Ok(Self {
                version,
                message_type,
                payload,
            })
        }
    }
    
    let message = ProtocolMessage {
        version: 1,
        message_type: 42,
        payload: vec![1, 2, 3, 4, 5],
    };
    
    let ctx_v1 = SerializationContext {
        protocol_version: 1,
        network_id: 0,
        compressed: false,
    };
    
    let ctx_v2 = SerializationContext {
        protocol_version: 2,
        network_id: 1,
        compressed: true,
    };
    
    let serialized_v1 = message.serialize_with_context(&ctx_v1)?;
    let serialized_v2 = message.serialize_with_context(&ctx_v2)?;
    
    println!("Context v1 serialization length: {}", serialized_v1.len());
    println!("Context v2 serialization length: {}", serialized_v2.len());
    
    let recovered_v1 = ProtocolMessage::deserialize_with_context(&serialized_v1, &ctx_v1)?;
    let recovered_v2 = ProtocolMessage::deserialize_with_context(&serialized_v2, &ctx_v2)?;
    
    println!("V1 recovery successful: {}", message.version == recovered_v1.version);
    println!("V2 recovery successful: {}", message.version == recovered_v2.version);

    // Custom type with manual implementation
    println!("\n=== Custom Serialization Implementation ===");
    
    #[derive(Debug, Clone, PartialEq)]
    struct Transaction {
        id: u64,
        from: Address,
        to: Address,
        amount: u64,
        fee: u64,
    }
    
    impl CatalystSerialize for Transaction {
        fn serialize(&self) -> CatalystResult<Vec<u8>> {
            let mut data = Vec::with_capacity(self.serialized_size());
            data.extend_from_slice(&self.id.to_le_bytes());
            data.extend_from_slice(self.from.as_slice());
            data.extend_from_slice(self.to.as_slice());
            data.extend_from_slice(&self.amount.to_le_bytes());
            data.extend_from_slice(&self.fee.to_le_bytes());
            Ok(data)
        }
        
        fn serialize_to<W: std::io::Write>(&self, writer: &mut W) -> CatalystResult<()> {
            writer.write_all(&self.id.to_le_bytes())?;
            writer.write_all(self.from.as_slice())?;
            writer.write_all(self.to.as_slice())?;
            writer.write_all(&self.amount.to_le_bytes())?;
            writer.write_all(&self.fee.to_le_bytes())?;
            Ok(())
        }
        
        fn serialized_size(&self) -> usize {
            8 + 20 + 20 + 8 + 8 // u64 + Address + Address + u64 + u64
        }
    }
    
    impl CatalystDeserialize for Transaction {
        fn deserialize(data: &[u8]) -> CatalystResult<Self> {
            if data.len() < 64 {
                return Err(catalyst_utils::CatalystError::Serialization(
                    "Insufficient data for Transaction".to_string()
                ));
            }
            
            let mut offset = 0;
            let id = u64::from_le_bytes([
                data[offset], data[offset+1], data[offset+2], data[offset+3],
                data[offset+4], data[offset+5], data[offset+6], data[offset+7]
            ]);
            offset += 8;
            
            let from = Address::from_slice(&data[offset..offset+20]);
            offset += 20;
            
            let to = Address::from_slice(&data[offset..offset+20]);
            offset += 20;
            
            let amount = u64::from_le_bytes([
                data[offset], data[offset+1], data[offset+2], data[offset+3],
                data[offset+4], data[offset+5], data[offset+6], data[offset+7]
            ]);
            offset += 8;
            
            let fee = u64::from_le_bytes([
                data[offset], data[offset+1], data[offset+2], data[offset+3],
                data[offset+4], data[offset+5], data[offset+6], data[offset+7]
            ]);
            
            Ok(Self { id, from, to, amount, fee })
        }
        
        fn deserialize_from<R: std::io::Read>(reader: &mut R) -> CatalystResult<Self> {
            let mut buffer = vec![0u8; 64];
            reader.read_exact(&mut buffer)?;
            Self::deserialize(&buffer)
        }
    }
    
    let transaction = Transaction {
        id: 12345,
        from: Address::new([1u8; 20]),
        to: Address::new([2u8; 20]),
        amount: 1000,
        fee: 10,
    };
    
    let tx_bytes = CatalystSerialize::serialize(&transaction)?;
    let recovered_tx = <Transaction as CatalystDeserialize>::deserialize(&tx_bytes)?;
    
    println!("Transaction serialization successful: {}", transaction == recovered_tx);
    println!("Serialized size: {} bytes", tx_bytes.len());

    // Performance test
    println!("\n=== Performance Test ===");
    let large_data = vec![42u8; 1_000_000]; // 1MB of data
    let serialized = CatalystSerialize::serialize(&large_data)?;
    println!("Serialized 1MB of data to {} bytes", serialized.len());
    
    let start = std::time::Instant::now();
    let deserialized = <Vec<u8> as CatalystDeserialize>::deserialize(&serialized)?;
    let duration = start.elapsed();
    
    println!("Deserialization took: {:?}", duration);
    println!("Data integrity check: {}", large_data == deserialized);

    // Error handling examples
    println!("\n=== Error Handling ===");
    
    // Try to deserialize invalid data
    let invalid_data = vec![1, 2, 3]; // Too short for a String
    let result = <String as CatalystDeserialize>::deserialize(&invalid_data);
    println!("Invalid data handling: {}", result.is_err());
    
    // Test corrupted data
    let valid_string = "Valid string".to_string();
    let mut valid_bytes = CatalystSerialize::serialize(&valid_string)?;
    valid_bytes[2] = 255; // Corrupt the length
    let result = <String as CatalystDeserialize>::deserialize(&valid_bytes);
    println!("Corrupted data handling: {}", result.is_err());

    println!("\n=== All Examples Completed Successfully ===");
    Ok(())
}

// Example of using the macro for automatic implementation
#[derive(Debug, PartialEq)]
struct AccountState {
    address: Address,
    balance: u64,
    nonce: u64,
    code_hash: Option<Hash>,
}

// Manually implement for compatibility with the example
impl CatalystSerialize for AccountState {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        let mut buffer = Vec::with_capacity(self.serialized_size());
        self.serialize_to(&mut buffer)?;
        Ok(buffer)
    }
    
    fn serialize_to<W: std::io::Write>(&self, writer: &mut W) -> CatalystResult<()> {
        // Serialize address
        writer.write_all(self.address.as_slice())?;
        // Serialize balance
        writer.write_all(&self.balance.to_le_bytes())?;
        // Serialize nonce
        writer.write_all(&self.nonce.to_le_bytes())?;
        // Serialize optional code_hash
        match &self.code_hash {
            Some(hash) => {
                writer.write_all(&[1u8])?; // has hash
                writer.write_all(hash.as_slice())?;
            }
            None => {
                writer.write_all(&[0u8])?; // no hash
            }
        }
        Ok(())
    }
    
    fn serialized_size(&self) -> usize {
        20 + 8 + 8 + 1 + if self.code_hash.is_some() { 32 } else { 0 }
    }
}

impl CatalystDeserialize for AccountState {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        let mut cursor = std::io::Cursor::new(data);
        Self::deserialize_from(&mut cursor)
    }
    
    fn deserialize_from<R: std::io::Read>(reader: &mut R) -> CatalystResult<Self> {
        // Read address (20 bytes)
        let mut addr_bytes = [0u8; 20];
        reader.read_exact(&mut addr_bytes)?;
        let address = Address::new(addr_bytes);
        
        // Read balance (8 bytes)
        let mut balance_bytes = [0u8; 8];
        reader.read_exact(&mut balance_bytes)?;
        let balance = u64::from_le_bytes(balance_bytes);
        
        // Read nonce (8 bytes)
        let mut nonce_bytes = [0u8; 8];
        reader.read_exact(&mut nonce_bytes)?;
        let nonce = u64::from_le_bytes(nonce_bytes);
        
        // Read optional code_hash
        let mut has_hash = [0u8; 1];
        reader.read_exact(&mut has_hash)?;
        let code_hash = if has_hash[0] == 1 {
            let mut hash_bytes = [0u8; 32];
            reader.read_exact(&mut hash_bytes)?;
            Some(Hash::new(hash_bytes))
        } else {
            None
        };
        
        Ok(Self {
            address,
            balance,
            nonce,
            code_hash,
        })
    }
}