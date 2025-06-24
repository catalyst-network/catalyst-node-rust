// examples/serialization_example.rs

use catalyst_utils::*;
use catalyst_utils::serialization::{self};

fn main() -> CatalystResult<()> {
    println!("=== Catalyst Serialization System Example ===\n");

    // 1. Basic type serialization
    println!("1. Basic type serialization:");
    
    // Primitive types
    let value_u8: u8 = 255;
    let value_u32: u32 = 0x12345678;
    let value_u64: u64 = 0x123456789ABCDEF0;
    
    println!("   Serializing primitive types:");
    println!("     u8 {} -> {} bytes", value_u8, value_u8.serialized_size());
    println!("     u32 {} -> {} bytes", value_u32, value_u32.serialized_size());
    println!("     u64 {} -> {} bytes", value_u64, value_u64.serialized_size());
    
    // Test round-trip
    let bytes_u32 = CatalystSerialize::serialize(&value_u32)?;
    let recovered_u32 = <u32 as CatalystDeserialize>::deserialize(&bytes_u32)?;
    assert_eq!(value_u32, recovered_u32);
    println!("     ✓ u32 round-trip successful");
    
    // Strings and bytes
    let test_string = "Catalyst Network Transaction".to_string();
    let test_bytes = vec![0xCA, 0xFE, 0xBA, 0xBE, 0xDE, 0xAD, 0xBE, 0xEF];
    
    println!("   Serializing collections:");
    println!("     String '{}' -> {} bytes", test_string, test_string.serialized_size());
    println!("     Vec<u8> {:?} -> {} bytes", test_bytes, test_bytes.serialized_size());
    
    let string_bytes = CatalystSerialize::serialize(&test_string)?;
    let recovered_string = <String as CatalystDeserialize>::deserialize(&string_bytes)?;
    assert_eq!(test_string, recovered_string);
    println!("     ✓ String round-trip successful");
    println!();

    // 2. Custom struct serialization
    println!("2. Custom struct serialization:");
    
    #[derive(Debug, PartialEq)]
    struct TransactionEntry {
        public_key: Vec<u8>,
        amount: u64,
        signature: Vec<u8>,
    }
    
    impl_catalyst_serialize!(TransactionEntry, public_key, amount, signature);
    
    let entry = TransactionEntry {
        public_key: vec![0x04; 32], // Mock public key
        amount: 1000000, // 1M units
        signature: vec![0x30; 64], // Mock signature
    };
    
    println!("   Transaction entry:");
    println!("     Amount: {}", entry.amount);
    println!("     Public key size: {} bytes", entry.public_key.len());
    println!("     Signature size: {} bytes", entry.signature.len());
    println!("     Total serialized size: {} bytes", entry.serialized_size());
    
    let entry_bytes = CatalystSerialize::serialize(&entry)?;
    let recovered_entry = <TransactionEntry as CatalystDeserialize>::deserialize(&entry_bytes)?;
    assert_eq!(entry, recovered_entry);
    println!("     ✓ TransactionEntry round-trip successful");
    println!();

    // 3. Context-aware serialization
    println!("3. Context-aware serialization:");
    
    #[derive(Debug, Clone)]
    struct ProtocolMessage {
        version: u32,
        message_type: u8,
        payload: Vec<u8>,
    }
    
    impl CatalystSerializeWithContext for ProtocolMessage {
        fn serialize_with_context(&self, ctx: &SerializationContext) -> CatalystResult<Vec<u8>> {
            let mut buffer = Vec::new();
            
            // Protocol version determines format
            if ctx.protocol_version >= 2 {
                // New format: version + network_id + message_type + payload
                buffer.extend_from_slice(&ctx.protocol_version.to_le_bytes());
                buffer.push(ctx.network_id);
                buffer.push(self.message_type);
                self.payload.serialize_to(&mut buffer)?;
            } else {
                // Legacy format: just message_type + payload
                buffer.push(self.message_type);
                self.payload.serialize_to(&mut buffer)?;
            }
            
            // Optional compression for large payloads
            if ctx.compressed && self.payload.len() > 1024 {
                println!("     (Would compress {} byte payload)", self.payload.len());
            }
            
            Ok(buffer)
        }
        
        fn deserialize_with_context(data: &[u8], ctx: &SerializationContext) -> CatalystResult<Self> {
            use std::io::Cursor;
            let mut cursor = Cursor::new(data);
            
            if ctx.protocol_version >= 2 {
                // New format
                let version = u32::deserialize_from(&mut cursor)?;
                let network_id = u8::deserialize_from(&mut cursor)?;
                let message_type = u8::deserialize_from(&mut cursor)?;
                let payload = Vec::<u8>::deserialize_from(&mut cursor)?;
                
                println!("     Deserialized v{} message (network: {}, type: {})", 
                    version, network_id, message_type);
                
                Ok(Self {
                    version,
                    message_type,
                    payload,
                })
            } else {
                // Legacy format
                let message_type = u8::deserialize_from(&mut cursor)?;
                let payload = Vec::<u8>::deserialize_from(&mut cursor)?;
                
                println!("     Deserialized legacy message (type: {})", message_type);
                
                Ok(Self {
                    version: 1,
                    message_type,
                    payload,
                })
            }
        }
    }
    
    let message = ProtocolMessage {
        version: 2,
        message_type: 42,
        payload: vec![1, 2, 3, 4, 5],
    };
    
    // Test with different contexts
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
    
    println!("   Testing context-aware serialization:");
    
    let v1_bytes = message.serialize_with_context(&ctx_v1)?;
    let v2_bytes = message.serialize_with_context(&ctx_v2)?;
    
    println!("     V1 format: {} bytes", v1_bytes.len());
    println!("     V2 format: {} bytes", v2_bytes.len());
    
    let _recovered_v1 = ProtocolMessage::deserialize_with_context(&v1_bytes, &ctx_v1)?;
    let _recovered_v2 = ProtocolMessage::deserialize_with_context(&v2_bytes, &ctx_v2)?;
    
    println!("     ✓ Context-aware serialization successful");
    println!();

    // 4. Complex Catalyst structures
    println!("4. Complex Catalyst structures:");
    
    // For custom structs - need to implement Vec<TransactionEntry> serialization
    #[derive(Debug, PartialEq)]
    struct Transaction {
        entries: Vec<TransactionEntry>,
        timestamp: u64,
        nonce: u64,
        signature: Vec<u8>,
    }
    
    // Manual implementation since Vec<TransactionEntry> needs custom handling
    impl CatalystSerialize for Transaction {
        fn serialize(&self) -> CatalystResult<Vec<u8>> {
            let mut buffer = Vec::with_capacity(self.serialized_size());
            self.serialize_to(&mut buffer)?;
            Ok(buffer)
        }
        
        fn serialize_to<W: std::io::Write>(&self, writer: &mut W) -> CatalystResult<()> {
            serialization::utils::serialize_vec(writer, &self.entries)?;
            self.timestamp.serialize_to(writer)?;
            self.nonce.serialize_to(writer)?;
            self.signature.serialize_to(writer)?;
            Ok(())
        }
        
        fn serialized_size(&self) -> usize {
            4 + // entries length
            self.entries.iter().map(|e| e.serialized_size()).sum::<usize>() +
            8 + // timestamp
            8 + // nonce
            self.signature.serialized_size()
        }
    }
    
    impl CatalystDeserialize for Transaction {
        fn deserialize(data: &[u8]) -> CatalystResult<Self> {
            let mut cursor = std::io::Cursor::new(data);
            Self::deserialize_from(&mut cursor)
        }
        
        fn deserialize_from<R: std::io::Read>(reader: &mut R) -> CatalystResult<Self> {
            let entries = serialization::utils::deserialize_vec(reader)?;
            let timestamp = u64::deserialize_from(reader)?;
            let nonce = u64::deserialize_from(reader)?;
            let signature = Vec::<u8>::deserialize_from(reader)?;
            
            Ok(Self {
                entries,
                timestamp,
                nonce,
                signature,
            })
        }
    }
    
    let transaction = Transaction {
        entries: vec![
            TransactionEntry {
                public_key: vec![0x01; 32],
                amount: 500000,
                signature: vec![0x11; 64],
            },
            TransactionEntry {
                public_key: vec![0x02; 32],
                amount: 300000,
                signature: vec![0x22; 64],
            },
        ],
        timestamp: catalyst_utils::utils::current_timestamp(),
        nonce: 12345,
        signature: vec![0xFF; 64],
    };
    
    println!("   Complex transaction:");
    println!("     Entries: {}", transaction.entries.len());
    println!("     Timestamp: {}", transaction.timestamp);
    println!("     Nonce: {}", transaction.nonce);
    println!("     Total serialized size: {} bytes", transaction.serialized_size());
    
    let tx_bytes = CatalystSerialize::serialize(&transaction)?;
    let recovered_tx = <Transaction as CatalystDeserialize>::deserialize(&tx_bytes)?;
    assert_eq!(transaction, recovered_tx);
    println!("     ✓ Complex transaction round-trip successful");
    println!();

    // 5. Performance testing
    println!("5. Performance testing:");
    
    let large_data = vec![0x42u8; 10000]; // 10KB of data
    
    let start = std::time::Instant::now();
    let serialized = CatalystSerialize::serialize(&large_data)?;
    let serialize_time = start.elapsed();
    
    let start = std::time::Instant::now();
    let deserialized = <Vec<u8> as CatalystDeserialize>::deserialize(&serialized)?;
    let deserialize_time = start.elapsed();
    
    assert_eq!(large_data, deserialized);
    
    println!("   Large data performance:");
    println!("     Data size: {} bytes", large_data.len());
    println!("     Serialized size: {} bytes (overhead: {} bytes)", 
        serialized.len(), serialized.len() - large_data.len());
    println!("     Serialize time: {:?}", serialize_time);
    println!("     Deserialize time: {:?}", deserialize_time);
    println!("     Total round-trip: {:?}", serialize_time + deserialize_time);
    println!();

    // 6. Error handling
    println!("6. Error handling:");
    
    // Test invalid data
    let invalid_data = vec![0xFF, 0xFF, 0xFF, 0xFF]; // Invalid length prefix
    let result = <String as CatalystDeserialize>::deserialize(&invalid_data);
    
    match result {
        Ok(_) => println!("     ❌ Expected error but got success"),
        Err(e) => println!("     ✓ Correctly handled invalid data: {}", e),
    }
    
    // Test truncated data
    let valid_string = "test".to_string();
    let mut valid_bytes = CatalystSerialize::serialize(&valid_string)?;
    valid_bytes.truncate(2); // Truncate to cause error
    
    let result = <String as CatalystDeserialize>::deserialize(&valid_bytes);
    match result {
        Ok(_) => println!("     ❌ Expected error but got success"),
        Err(e) => println!("     ✓ Correctly handled truncated data: {}", e),
    }
    println!();

    // 7. Utility function usage
    println!("7. Utility function usage:");
        
    // Manual serialization using utilities
    let mut buffer = Vec::new();
    serialization::utils::serialize_string(&mut buffer, "Catalyst")?;
    serialization::utils::serialize_bytes(&mut buffer, &[0xCA, 0xFE])?;
    
    println!("   Manual serialization buffer: {} bytes", buffer.len());
    
    // Manual deserialization
    let mut cursor = std::io::Cursor::new(&buffer);
    let recovered_string = serialization::utils::deserialize_string(&mut cursor)?;
    let recovered_bytes = serialization::utils::deserialize_bytes(&mut cursor)?;
    
    assert_eq!(recovered_string, "Catalyst");
    assert_eq!(recovered_bytes, vec![0xCA, 0xFE]);
    
    println!("     ✓ Manual serialization utilities work correctly");
    println!();

    println!("=== Serialization example completed successfully! ===");
    Ok(())
}

// Example of implementing serialization for account states
#[derive(Debug, PartialEq)]
struct AccountState {
    address: [u8; 21],
    balance: u64,
    nonce: u64,
    data: Option<Vec<u8>>,
}

impl CatalystSerialize for AccountState {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        let mut buffer = Vec::with_capacity(self.serialized_size());
        self.serialize_to(&mut buffer)?;
        Ok(buffer)
    }
    
    fn serialize_to<W: std::io::Write>(&self, writer: &mut W) -> CatalystResult<()> {
        // Address (fixed 21 bytes)
        writer.write_all(&self.address)
            .map_err(|e| CatalystError::Serialization(format!("Failed to write address: {}", e)))?;
        
        // Balance and nonce
        self.balance.serialize_to(writer)?;
        self.nonce.serialize_to(writer)?;
        
        // Optional data
        match &self.data {
            Some(data) => {
                1u8.serialize_to(writer)?; // Has data flag
                data.serialize_to(writer)?;
            }
            None => {
                0u8.serialize_to(writer)?; // No data flag
            }
        }
        
        Ok(())
    }
    
    fn serialized_size(&self) -> usize {
        21 + // address
        8 + // balance
        8 + // nonce
        1 + // data flag
        self.data.as_ref().map(|d| 4 + d.len()).unwrap_or(0) // optional data with length prefix
    }
}

impl CatalystDeserialize for AccountState {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        let mut cursor = std::io::Cursor::new(data);
        Self::deserialize_from(&mut cursor)
    }
    
    fn deserialize_from<R: std::io::Read>(reader: &mut R) -> CatalystResult<Self> {
        // Address
        let mut address = [0u8; 21];
        reader.read_exact(&mut address)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read address: {}", e)))?;
        
        // Balance and nonce
        let balance = u64::deserialize_from(reader)?;
        let nonce = u64::deserialize_from(reader)?;
        
        // Optional data
        let has_data = u8::deserialize_from(reader)?;
        let data = if has_data == 1 {
            Some(Vec::<u8>::deserialize_from(reader)?)
        } else {
            None
        };
        
        Ok(Self {
            address,
            balance,
            nonce,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_examples() {
        assert!(main().is_ok());
    }
    
    #[test]
    fn test_account_state_serialization() {
        let account = AccountState {
            address: [1u8; 21],
            balance: 1000000,
            nonce: 42,
            data: Some(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        };
        
        let serialized = CatalystSerialize::serialize(&account).unwrap();
        let deserialized = <AccountState as CatalystDeserialize>::deserialize(&serialized).unwrap();
        
        assert_eq!(account, deserialized);
    }
    
    #[test]
    fn test_empty_account_serialization() {
        let account = AccountState {
            address: [0u8; 21],
            balance: 0,
            nonce: 0,
            data: None,
        };
        
        let serialized = CatalystSerialize::serialize(&account).unwrap();
        let deserialized = <AccountState as CatalystDeserialize>::deserialize(&serialized).unwrap();
        
        assert_eq!(account, deserialized);
    }
}