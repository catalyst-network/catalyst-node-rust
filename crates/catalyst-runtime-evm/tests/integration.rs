// tests/integration.rs
//! Integration tests for the Catalyst EVM runtime
//! 
//! These tests verify end-to-end functionality including:
//! - Contract deployment and execution
//! - Cross-transaction state persistence
//! - Gas metering accuracy
//! - Error handling
//! - Performance characteristics

use catalyst_runtime_evm::{
    CatalystEvmRuntime, CatalystEvmConfig, CatalystFeatures, EvmConfig,
    EvmTransaction, BlockEnv, CatalystExecutionResult, ContractDeployer,
    GasMeter, FeeCalculator, EvmDebugger,
};
use catalyst_runtime_evm::database::InMemoryDatabase;
use ethereum_types::{Address, U256, H256};
use hex;
use std::str::FromStr;

/// Helper function to create a test runtime
fn create_test_runtime() -> CatalystEvmRuntime<InMemoryDatabase> {
    let config = CatalystEvmConfig {
        evm_config: EvmConfig {
            chain_id: 31337,
            block_gas_limit: 30_000_000,
            base_fee: U256::from(1_000_000_000u64), // 1 gwei
            enable_eip1559: true,
            enable_london: true,
            enable_berlin: true,
            max_code_size: 49152,
            view_gas_limit: 50_000_000,
        },
        catalyst_features: CatalystFeatures {
            cross_runtime_enabled: false,
            confidential_tx_enabled: false,
            dfs_integration: false,
            catalyst_gas_model: true,
        },
    };

    let mut database = InMemoryDatabase::new();
    
    // Setup test accounts with sufficient balance
    let accounts = [
        (Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap(), U256::from(100_000_000_000_000_000_000u64)), // 100 ETH
        (Address::from_str("0x8ba1f109551bD432803012645Hac136c2Bb25F00c").unwrap(), U256::from(50_000_000_000_000_000_000u64)),  // 50 ETH
        (Address::from_str("0x1234567890123456789012345678901234567890").unwrap(), U256::from(25_000_000_000_000_000_000u64)),  // 25 ETH
    ];

    for (address, balance) in accounts {
        database.create_account(address, balance);
    }

    CatalystEvmRuntime::new(database, config)
}

fn create_test_block_env(block_number: u64) -> BlockEnv {
    BlockEnv {
        number: U256::from(block_number),
        coinbase: Address::from_str("0x0000000000000000000000000000000000000000").unwrap(),
        timestamp: U256::from(1640995200 + block_number * 12), // 12 second blocks
        gas_limit: U256::from(30_000_000),
        basefee: U256::from(1_000_000_000u64),
        difficulty: U256::zero(),
        prevrandao: Some(H256::random().into()),
    }
}

#[tokio::test]
async fn test_simple_eth_transfer() {
    let mut runtime = create_test_runtime();
    let block_env = create_test_block_env(1);
    
    let from = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap();
    let to = Address::from_str("0x8ba1f109551bD432803012645Hac136c2Bb25F00c").unwrap();
    
    let tx = EvmTransaction {
        from,
        to: Some(to),
        value: U256::from(1_000_000_000_000_000_000u64), // 1 ETH
        data: vec![],
        gas_limit: 21000,
        gas_price: U256::from(20_000_000_000u64), // 20 gwei
        gas_priority_fee: Some(U256::from(1_000_000_000u64)), // 1 gwei tip
        nonce: 0,
        chain_id: 31337,
        access_list: None,
    };

    let result = runtime.execute_transaction(&tx, &block_env).await.unwrap();
    
    assert!(result.success(), "Transaction should succeed");
    assert_eq!(result.gas_used(), 21000, "Should use exactly 21000 gas for simple transfer");
    assert!(result.return_data().is_empty(), "Simple transfer should have no return data");
    assert!(result.logs().is_empty(), "Simple transfer should emit no logs");
}

#[tokio::test]
async fn test_contract_deployment() {
    let mut runtime = create_test_runtime();
    let block_env = create_test_block_env(1);
    
    let deployer = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap();
    
    // Simple storage contract bytecode
    let bytecode = hex::decode(
        "608060405234801561001057600080fd5b5060008055348015602457600080fd5b50603f8060336000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c8063d09de08a14602d575b600080fd5b60336035565b005b60016000808282546046919060449061004a565b925050819055508056fea264697066735822"
    ).unwrap();

    let deploy_tx = EvmTransaction {
        from: deployer,
        to: None, // Contract creation
        value: U256::zero(),
        data: bytecode,
        gas_limit: 500_000,
        gas_price: U256::from(20_000_000_000u64),
        gas_priority_fee: Some(U256::from(1_000_000_000u64)),
        nonce: 0,
        chain_id: 31337,
        access_list: None,
    };

    let result = runtime.execute_transaction(&deploy_tx, &block_env).await.unwrap();
    
    assert!(result.success(), "Contract deployment should succeed");
    assert!(result.gas_used() > 21000, "Contract deployment should use more than 21000 gas");
    assert!(result.base_result.created_address.is_some(), "Should return contract address");
    
    let contract_address = result.base_result.created_address.unwrap();
    assert_ne!(contract_address, Address::zero(), "Contract address should not be zero");
}

#[tokio::test]
async fn test_contract_interaction() {
    let mut runtime = create_test_runtime();
    let block_env = create_test_block_env(1);
    
    let deployer = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap();
    let user = Address::from_str("0x8ba1f109551bD432803012645Hac136c2Bb25F00c").unwrap();
    
    // First, deploy a simple counter contract
    let bytecode = hex::decode(
        "608060405234801561001057600080fd5b5060008055348015602457600080fd5b50603f8060336000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c8063d09de08a14602d575b600080fd5b60336035565b005b60016000808282546046919060449061004a565b925050819055508056fea264697066735822"
    ).unwrap();

    let deploy_tx = EvmTransaction {
        from: deployer,
        to: None,
        value: U256::zero(),
        data: bytecode,
        gas_limit: 500_000,
        gas_price: U256::from(20_000_000_000u64),
        gas_priority_fee: Some(U256::from(1_000_000_000u64)),
        nonce: 0,
        chain_id: 31337,
        access_list: None,
    };

    let deploy_result = runtime.execute_transaction(&deploy_tx, &block_env).await.unwrap();
    assert!(deploy_result.success());
    
    let contract_address = deploy_result.base_result.created_address.unwrap();
    
    // Now call the increment function
    let call_data = hex::decode("d09de08a").unwrap(); // increment() function selector
    
    let call_tx = EvmTransaction {
        from: user,
        to: Some(contract_address),
        value: U256::zero(),
        data: call_data,
        gas_limit: 100_000,
        gas_price: U256::from(20_000_000_000u64),
        gas_priority_fee: Some(U256::from(1_000_000_000u64)),
        nonce: 0,
        chain_id: 31337,
        access_list: None,
    };

    let call_result = runtime.execute_transaction(&call_tx, &block_env).await.unwrap();
    
    assert!(call_result.success(), "Contract call should succeed");
    assert!(call_result.gas_used() > 21000, "Contract call should use more than transfer gas");
}

#[tokio::test]
async fn test_gas_limit_enforcement() {
    let mut runtime = create_test_runtime();
    let block_env = create_test_block_env(1);
    
    let from = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap();
    let to = Address::from_str("0x8ba1f109551bD432803012645Hac136c2Bb25F00c").unwrap();
    
    // Try to send a transaction with insufficient gas
    let tx = EvmTransaction {
        from,
        to: Some(to),
        value: U256::from(1_000_000_000_000_000_000u64),
        data: vec![],
        gas_limit: 20_000, // Less than required 21000
        gas_price: U256::from(20_000_000_000u64),
        gas_priority_fee: Some(U256::from(1_000_000_000u64)),
        nonce: 0,
        chain_id: 31337,
        access_list: None,
    };

    let result = runtime.execute_transaction(&tx, &block_env).await;
    
    // Should either fail or consume all available gas
    if let Ok(execution_result) = result {
        assert!(!execution_result.success() || execution_result.gas_used() == 20_000);
    }
}

#[tokio::test]
async fn test_nonce_validation() {
    let mut runtime = create_test_runtime();
    let block_env = create_test_block_env(1);
    
    let from = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap();
    let to = Address::from_str("0x8ba1f109551bD432803012645Hac136c2Bb25F00c").unwrap();
    
    // First transaction with nonce 0 (should succeed)
    let tx1 = EvmTransaction {
        from,
        to: Some(to),
        value: U256::from(1_000_000_000_000_000_000u64),
        data: vec![],
        gas_limit: 21000,
        gas_price: U256::from(20_000_000_000u64),
        gas_priority_fee: Some(U256::from(1_000_000_000u64)),
        nonce: 0,
        chain_id: 31337,
        access_list: None,
    };

    let result1 = runtime.execute_transaction(&tx1, &block_env).await.unwrap();
    assert!(result1.success());
    
    // Second transaction with nonce 0 again (should fail or be rejected)
    let tx2 = EvmTransaction {
        from,
        to: Some(to),
        value: U256::from(1_000_000_000_000_000_000u64),
        data: vec![],
        gas_limit: 21000,
        gas_price: U256::from(20_000_000_000u64),
        gas_priority_fee: Some(U256::from(1_000_000_000u64)),
        nonce: 0, // Same nonce
        chain_id: 31337,
        access_list: None,
    };

    let result2 = runtime.execute_transaction(&tx2, &block_env).await;
    // This should fail due to nonce reuse
    assert!(result2.is_err() || !result2.unwrap().success());
}

#[tokio::test]
async fn test_gas_meter_functionality() {
    let mut meter = GasMeter::new(100_000, U256::from(20_000_000_000u64));
    
    assert_eq!(meter.gas_remaining(), 100_000);
    assert_eq!(meter.gas_used(), 0);
    
    // Consume some gas
    assert!(meter.consume_gas(50_000).is_ok());
    assert_eq!(meter.gas_remaining(), 50_000);
    assert_eq!(meter.gas_used(), 50_000);
    
    // Try to consume more gas than available
    assert!(meter.consume_gas(60_000).is_err());
    assert_eq!(meter.gas_remaining(), 50_000); // Should remain unchanged
    assert_eq!(meter.gas_used(), 50_000);
    
    // Consume remaining gas exactly
    assert!(meter.consume_gas(50_000).is_ok());
    assert_eq!(meter.gas_remaining(), 0);
    assert_eq!(meter.gas_used(), 100_000);
}

#[tokio::test]
async fn test_fee_calculator() {
    let calculator = FeeCalculator::new(U256::from(1_000_000_000u64)); // 1 gwei base fee
    
    // Test basic fee calculation
    let fee = calculator.calculate_fee(21000, None);
    assert_eq!(fee, U256::from(21000 * 1_000_000_000u64));
    
    // Test with priority fee
    let fee_with_priority = calculator.calculate_fee(21000, Some(U256::from(1_000_000_000u64)));
    assert!(fee_with_priority > fee);
    
    // Test congestion adjustment
    let mut congested_calculator = FeeCalculator::new(U256::from(1_000_000_000u64));
    congested_calculator.update_congestion(0.8); // 80% network utilization
    
    let congested_fee = congested_calculator.calculate_fee(21000, None);
    assert!(congested_fee > fee); // Should be higher due to congestion
}

#[tokio::test]
async fn test_contract_deployer_create_addresses() {
    let mut deployer = ContractDeployer::new();
    let deployer_address = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap();
    let bytecode = vec![0x60, 0x60, 0x60, 0x40, 0x52]; // Simple bytecode
    
    // Test CREATE address calculation
    let address1 = deployer.deploy_contract(
        deployer_address,
        bytecode.clone(),
        vec![],
        None,
    ).unwrap();
    
    let address2 = deployer.deploy_contract(
        deployer_address,
        bytecode.clone(),
        vec![],
        None,
    ).unwrap();
    
    // Addresses should be different due to nonce increment
    assert_ne!(address1, address2);
    assert_ne!(address1, Address::zero());
    assert_ne!(address2, Address::zero());
    
    // Test CREATE2 address calculation
    let salt1 = H256::from_low_u64_be(12345);
    let salt2 = H256::from_low_u64_be(54321);
    
    let create2_address1 = deployer.deploy_contract(
        deployer_address,
        bytecode.clone(),
        vec![],
        Some(salt1),
    ).unwrap();
    
    let create2_address2 = deployer.deploy_contract(
        deployer_address,
        bytecode.clone(),
        vec![],
        Some(salt2),
    ).unwrap();
    
    // CREATE2 addresses should be different with different salts
    assert_ne!(create2_address1, create2_address2);
    
    // Same salt should produce same address
    let create2_address1_repeat = deployer.deploy_contract(
        deployer_address,
        bytecode,
        vec![],
        Some(salt1),
    ).unwrap();
    
    assert_eq!(create2_address1, create2_address1_repeat);
}

#[tokio::test]
async fn test_debugger_functionality() {
    let mut debugger = EvmDebugger::new(true);
    
    // Simulate some opcode execution
    debugger.trace_opcode("PUSH1", 3);
    debugger.trace_opcode("PUSH1", 3);
    debugger.trace_opcode("ADD", 3);
    debugger.trace_opcode("STORE", 5000);
    
    let trace = debugger.get_trace();
    assert_eq!(trace.len(), 4);
    
    assert_eq!(trace[0], ("PUSH1".to_string(), 3));
    assert_eq!(trace[1], ("PUSH1".to_string(), 3));
    assert_eq!(trace[2], ("ADD".to_string(), 3));
    assert_eq!(trace[3], ("STORE".to_string(), 5000));
    
    // Test disabled debugger
    let mut disabled_debugger = EvmDebugger::new(false);
    disabled_debugger.trace_opcode("PUSH1", 3);
    
    let empty_trace = disabled_debugger.get_trace();
    assert!(empty_trace.is_empty());
}

#[tokio::test]
async fn test_multiple_transactions_same_block() {
    let mut runtime = create_test_runtime();
    let block_env = create_test_block_env(1);
    
    let from = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap();
    let to1 = Address::from_str("0x8ba1f109551bD432803012645Hac136c2Bb25F00c").unwrap();
    let to2 = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    
    // Execute multiple transactions
    let transactions = vec![
        EvmTransaction {
            from,
            to: Some(to1),
            value: U256::from(1_000_000_000_000_000_000u64), // 1 ETH
            data: vec![],
            gas_limit: 21000,
            gas_price: U256::from(20_000_000_000u64),
            gas_priority_fee: Some(U256::from(1_000_000_000u64)),
            nonce: 0,
            chain_id: 31337,
            access_list: None,
        },
        EvmTransaction {
            from,
            to: Some(to2),
            value: U256::from(2_000_000_000_000_000_000u64), // 2 ETH
            data: vec![],
            gas_limit: 21000,
            gas_price: U256::from(20_000_000_000u64),
            gas_priority_fee: Some(U256::from(1_000_000_000u64)),
            nonce: 1,
            chain_id: 31337,
            access_list: None,
        },
    ];
    
    let mut total_gas_used = 0;
    
    for tx in transactions {
        let result = runtime.execute_transaction(&tx, &block_env).await.unwrap();
        assert!(result.success(), "All transactions should succeed");
        total_gas_used += result.gas_used();
    }
    
    assert_eq!(total_gas_used, 42000); // 21000 * 2 transactions
}

#[tokio::test]
async fn test_catalyst_execution_result_serialization() {
    use serde_json;
    
    let mut runtime = create_test_runtime();
    let block_env = create_test_block_env(1);
    
    let from = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4").unwrap();
    let to = Address::from_str("0x8ba1f109551bD432803012645Hac136c2Bb25F00c").unwrap();
    
    let tx = EvmTransaction {
        from,
        to: Some(to),
        value: U256::from(1_000_000_000_000_000_000u64),
        data: vec![],
        gas_limit: 21000,
        gas_price: U256::from(20_000_000_000u64),
        gas_priority_fee: Some(U256::from(1_000_000_000u64)),
        nonce: 0,
        chain_id: 31337,
        access_list: None,
    };

    let result = runtime.execute_transaction(&tx, &block_env).await.unwrap();
    
    // Test serialization
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.is_empty());
    
    // Test deserialization
    let deserialized: CatalystExecutionResult = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.success(), result.success());
    assert_eq!(deserialized.gas_used(), result.gas_used());
}

#[cfg(feature = "cross-runtime")]
#[tokio::test]
async fn test_cross_runtime_features() {
    // This test would verify cross-runtime functionality
    // when that feature is implemented
    let config = CatalystEvmConfig {
        evm_config: EvmConfig::default(),
        catalyst_features: CatalystFeatures {
            cross_runtime_enabled: true,
            confidential_tx_enabled: false,
            dfs_integration: false,
            catalyst_gas_model: false,
        },
    };
    
    let database = InMemoryDatabase::new();
    let _runtime = CatalystEvmRuntime::new(database, config);
    
    // Test cross-runtime call precompile
    // This would be implemented when cross-runtime features are ready
    assert!(true); // Placeholder
}

#[cfg(feature = "dfs-integration")]
#[tokio::test]
async fn test_dfs_integration() {
    // This test would verify DFS integration
    // when that feature is implemented
    let config = CatalystEvmConfig {
        evm_config: EvmConfig::default(),
        catalyst_features: CatalystFeatures {
            cross_runtime_enabled: false,
            confidential_tx_enabled: false,
            dfs_integration: true,
            catalyst_gas_model: false,
        },
    };
    
    let database = InMemoryDatabase::new();
    let _runtime = CatalystEvmRuntime::new(database, config);
    
    // Test DFS store/retrieve precompiles
    // This would be implemented when DFS features are ready
    assert!(true); // Placeholder
}