// catalyst-utils/src/serialization.rs

use crate::{CatalystError, CatalystResult};
use std::io::{Cursor, Read, Write};

/// Core serialization trait for Catalyst protocol messages
///
/// This trait provides a unified interface for serializing data structures
/// used across the Catalyst network, including transactions, consensus messages,
/// and ledger state updates.
pub trait CatalystSerialize {
    /// Serialize the object into bytes
    fn serialize(&self) -> CatalystResult<Vec<u8>>;

    /// Serialize the object directly to a writer
    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()>;

    /// Get the serialized size without actually serializing
    /// Useful for pre-allocating buffers and network message sizing
    fn serialized_size(&self) -> usize;
}

/// Core deserialization trait for Catalyst protocol messages
pub trait CatalystDeserialize: Sized {
    /// Deserialize from bytes
    fn deserialize(data: &[u8]) -> CatalystResult<Self>;

    /// Deserialize from a reader
    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self>;
}

/// Combined trait for types that can both serialize and deserialize
pub trait CatalystCodec: CatalystSerialize + CatalystDeserialize {}

// Automatically implement CatalystCodec for types that implement both traits
impl<T> CatalystCodec for T where T: CatalystSerialize + CatalystDeserialize {}

/// Serialization context for handling different protocol versions
/// and network-specific serialization parameters
#[derive(Debug, Clone)]
pub struct SerializationContext {
    /// Protocol version for backwards compatibility
    pub protocol_version: u32,
    /// Network ID (mainnet, testnet, etc.)
    pub network_id: u8,
    /// Whether to use compressed encoding where possible
    pub compressed: bool,
}

impl Default for SerializationContext {
    fn default() -> Self {
        Self {
            protocol_version: 1,
            network_id: 0, // mainnet
            compressed: true,
        }
    }
}

/// Helper trait for types that need context-aware serialization
pub trait CatalystSerializeWithContext {
    fn serialize_with_context(&self, ctx: &SerializationContext) -> CatalystResult<Vec<u8>>;
    fn deserialize_with_context(data: &[u8], ctx: &SerializationContext) -> CatalystResult<Self>
    where
        Self: Sized;
}

/// Utility functions for common serialization patterns
pub mod utils {
    use super::*;

    /// Serialize a length-prefixed byte array
    /// Format: [4-byte length][data]
    pub fn serialize_bytes<W: Write>(writer: &mut W, data: &[u8]) -> CatalystResult<()> {
        let len = data.len() as u32;
        writer
            .write_all(&len.to_le_bytes())
            .map_err(|e| CatalystError::Serialization(format!("Failed to write length: {}", e)))?;
        writer
            .write_all(data)
            .map_err(|e| CatalystError::Serialization(format!("Failed to write data: {}", e)))?;
        Ok(())
    }

    /// Deserialize a length-prefixed byte array
    pub fn deserialize_bytes<R: Read>(reader: &mut R) -> CatalystResult<Vec<u8>> {
        let mut len_bytes = [0u8; 4];
        reader
            .read_exact(&mut len_bytes)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read length: {}", e)))?;

        let len = u32::from_le_bytes(len_bytes) as usize;

        // Sanity check to prevent excessive memory allocation
        if len > 100_000_000 {
            // 100MB limit
            return Err(CatalystError::Serialization(format!(
                "Data length {} exceeds maximum allowed size",
                len
            )));
        }

        let mut data = vec![0u8; len];
        reader
            .read_exact(&mut data)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read data: {}", e)))?;

        Ok(data)
    }

    /// Serialize a string with length prefix
    pub fn serialize_string<W: Write>(writer: &mut W, s: &str) -> CatalystResult<()> {
        serialize_bytes(writer, s.as_bytes())
    }

    /// Deserialize a length-prefixed string
    pub fn deserialize_string<R: Read>(reader: &mut R) -> CatalystResult<String> {
        let bytes = deserialize_bytes(reader)?;
        String::from_utf8(bytes)
            .map_err(|e| CatalystError::Serialization(format!("Invalid UTF-8: {}", e)))
    }

    /// Serialize a vector of serializable items
    pub fn serialize_vec<W: Write, T: CatalystSerialize>(
        writer: &mut W,
        items: &[T],
    ) -> CatalystResult<()> {
        let len = items.len() as u32;
        writer.write_all(&len.to_le_bytes()).map_err(|e| {
            CatalystError::Serialization(format!("Failed to write vec length: {}", e))
        })?;

        for item in items {
            item.serialize_to(writer)?;
        }
        Ok(())
    }

    /// Deserialize a vector of deserializable items
    pub fn deserialize_vec<R: Read, T: CatalystDeserialize>(
        reader: &mut R,
    ) -> CatalystResult<Vec<T>> {
        let mut len_bytes = [0u8; 4];
        reader.read_exact(&mut len_bytes).map_err(|e| {
            CatalystError::Serialization(format!("Failed to read vec length: {}", e))
        })?;

        let len = u32::from_le_bytes(len_bytes) as usize;

        // Sanity check for vector length
        if len > 1_000_000 {
            // 1M items max
            return Err(CatalystError::Serialization(format!(
                "Vector length {} exceeds maximum allowed size",
                len
            )));
        }

        let mut items = Vec::with_capacity(len);
        for _ in 0..len {
            items.push(T::deserialize_from(reader)?);
        }

        Ok(items)
    }
}

// Implementations for common types

impl CatalystSerialize for u8 {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        Ok(vec![*self])
    }

    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()> {
        writer
            .write_all(&[*self])
            .map_err(|e| CatalystError::Serialization(format!("Failed to write u8: {}", e)))
    }

    fn serialized_size(&self) -> usize {
        1
    }
}

impl CatalystDeserialize for u8 {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        if data.len() < 1 {
            return Err(CatalystError::Serialization(
                "Insufficient data for u8".to_string(),
            ));
        }
        Ok(data[0])
    }

    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self> {
        let mut byte = [0u8; 1];
        reader
            .read_exact(&mut byte)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read u8: {}", e)))?;
        Ok(byte[0])
    }
}

impl CatalystSerialize for u32 {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        Ok(self.to_le_bytes().to_vec())
    }

    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()> {
        writer
            .write_all(&self.to_le_bytes())
            .map_err(|e| CatalystError::Serialization(format!("Failed to write u32: {}", e)))
    }

    fn serialized_size(&self) -> usize {
        4
    }
}

impl CatalystDeserialize for u32 {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        if data.len() < 4 {
            return Err(CatalystError::Serialization(
                "Insufficient data for u32".to_string(),
            ));
        }
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&data[0..4]);
        Ok(u32::from_le_bytes(bytes))
    }

    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self> {
        let mut bytes = [0u8; 4];
        reader
            .read_exact(&mut bytes)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read u32: {}", e)))?;
        Ok(u32::from_le_bytes(bytes))
    }
}

impl CatalystSerialize for u64 {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        Ok(self.to_le_bytes().to_vec())
    }

    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()> {
        writer
            .write_all(&self.to_le_bytes())
            .map_err(|e| CatalystError::Serialization(format!("Failed to write u64: {}", e)))
    }

    fn serialized_size(&self) -> usize {
        8
    }
}

impl CatalystDeserialize for u64 {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        if data.len() < 8 {
            return Err(CatalystError::Serialization(
                "Insufficient data for u64".to_string(),
            ));
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&data[0..8]);
        Ok(u64::from_le_bytes(bytes))
    }

    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self> {
        let mut bytes = [0u8; 8];
        reader
            .read_exact(&mut bytes)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read u64: {}", e)))?;
        Ok(u64::from_le_bytes(bytes))
    }
}

impl CatalystSerialize for Vec<u8> {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        let mut result = Vec::with_capacity(4 + self.len());
        self.serialize_to(&mut result)?;
        Ok(result)
    }

    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()> {
        utils::serialize_bytes(writer, self)
    }

    fn serialized_size(&self) -> usize {
        4 + self.len() // length prefix + data
    }
}

impl CatalystDeserialize for Vec<u8> {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        let mut cursor = Cursor::new(data);
        utils::deserialize_bytes(&mut cursor)
    }

    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self> {
        utils::deserialize_bytes(reader)
    }
}

impl CatalystSerialize for String {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        let mut result = Vec::with_capacity(4 + self.len());
        self.serialize_to(&mut result)?;
        Ok(result)
    }

    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()> {
        utils::serialize_string(writer, self)
    }

    fn serialized_size(&self) -> usize {
        4 + self.len() // length prefix + UTF-8 bytes
    }
}

impl CatalystDeserialize for String {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        let mut cursor = Cursor::new(data);
        utils::deserialize_string(&mut cursor)
    }

    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self> {
        utils::deserialize_string(reader)
    }
}

// Helper macro for implementing serialization for simple structs
#[macro_export]
macro_rules! impl_catalyst_serialize {
    ($type:ty, $($field:ident),*) => {
        impl $crate::serialization::CatalystSerialize for $type {
            fn serialize(&self) -> $crate::error::CatalystResult<Vec<u8>> {
                let mut buffer = Vec::with_capacity(self.serialized_size());
                self.serialize_to(&mut buffer)?;
                Ok(buffer)
            }

            fn serialize_to<W: std::io::Write>(&self, writer: &mut W) -> $crate::error::CatalystResult<()> {
                $(
                    self.$field.serialize_to(writer)?;
                )*
                Ok(())
            }

            fn serialized_size(&self) -> usize {
                0 $(+ self.$field.serialized_size())*
            }
        }

        impl $crate::serialization::CatalystDeserialize for $type {
            fn deserialize(data: &[u8]) -> $crate::error::CatalystResult<Self> {
                let mut cursor = std::io::Cursor::new(data);
                Self::deserialize_from(&mut cursor)
            }

            fn deserialize_from<R: std::io::Read>(reader: &mut R) -> $crate::error::CatalystResult<Self> {
                Ok(Self {
                    $(
                        $field: $crate::serialization::CatalystDeserialize::deserialize_from(reader)?,
                    )*
                })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_types_serialization() {
        // Test u8
        let val_u8: u8 = 42;
        let serialized = val_u8.serialize().unwrap();
        let deserialized = u8::deserialize(&serialized).unwrap();
        assert_eq!(val_u8, deserialized);

        // Test u32
        let val_u32: u32 = 0x12345678;
        let serialized = val_u32.serialize().unwrap();
        let deserialized = u32::deserialize(&serialized).unwrap();
        assert_eq!(val_u32, deserialized);

        // Test u64
        let val_u64: u64 = 0x123456789ABCDEF0;
        let serialized = val_u64.serialize().unwrap();
        let deserialized = u64::deserialize(&serialized).unwrap();
        assert_eq!(val_u64, deserialized);
    }

    #[test]
    fn test_string_serialization() {
        let original = "Hello, Catalyst Network!".to_string();
        let serialized = original.serialize().unwrap();
        let deserialized = String::deserialize(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_bytes_serialization() {
        let original = vec![1, 2, 3, 4, 5, 255, 0, 128];
        let serialized = original.serialize().unwrap();
        let deserialized = Vec::<u8>::deserialize(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_empty_data() {
        let empty_vec: Vec<u8> = vec![];
        let serialized = empty_vec.serialize().unwrap();
        let deserialized = Vec::<u8>::deserialize(&serialized).unwrap();
        assert_eq!(empty_vec, deserialized);

        let empty_string = String::new();
        let serialized = empty_string.serialize().unwrap();
        let deserialized = String::deserialize(&serialized).unwrap();
        assert_eq!(empty_string, deserialized);
    }

    #[test]
    fn test_serialization_context() {
        let ctx = SerializationContext::default();
        assert_eq!(ctx.protocol_version, 1);
        assert_eq!(ctx.network_id, 0);
        assert!(ctx.compressed);
    }

    #[test]
    fn test_size_limits() {
        // Test that we properly reject oversized data
        let mut fake_large_data = vec![255u8; 8]; // Fake a length of u32::MAX
        fake_large_data[0] = 0;
        fake_large_data[1] = 0;
        fake_large_data[2] = 0;
        fake_large_data[3] = 100; // 100MB + some bytes, should exceed limit

        let result = Vec::<u8>::deserialize(&fake_large_data);
        assert!(result.is_err());
    }

    // Example struct to test the macro
    #[derive(Debug, PartialEq)]
    struct TestStruct {
        id: u32,
        name: String,
        data: Vec<u8>,
    }

    impl_catalyst_serialize!(TestStruct, id, name, data);

    #[test]
    fn test_macro_serialization() {
        let original = TestStruct {
            id: 42,
            name: "test".to_string(),
            data: vec![1, 2, 3, 4],
        };

        let serialized = original.serialize().unwrap();
        let deserialized = TestStruct::deserialize(&serialized).unwrap();

        assert_eq!(original, deserialized);
    }
}
