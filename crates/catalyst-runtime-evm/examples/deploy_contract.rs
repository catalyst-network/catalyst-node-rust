// examples/deploy_contract.rs
//! Example demonstrating how to deploy and interact with a smart contract
//! 
//! This example shows:
//! - Setting up the Catalyst EVM runtime
//! - Deploying a simple smart contract
//! - Calling contract methods
//! - Handling execution results

use catalyst_runtime_evm::{
    CatalystEvmRuntime, CatalystEvmConfig, CatalystFeatures, EvmConfig,
    EvmTransaction, ContractDeployer, BlockEnv
};
use catalyst_runtime_evm::database::InMemoryDatabase;
use ethereum_types::{Address, U256, H256};
use hex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 Catalyst EVM Contract Deployment Example");

    // 1. Setup the EVM runtime
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

    // Create database and runtime
    let mut database = InMemoryDatabase::new();
    
    // Setup test accounts with initial balances
    let deployer_address = Address::from_str("0x742d35Cc6527C3c7Bb91B6B4d0e71b46C7E3e8F4")?;
    let user_address = Address::from_str("0x8ba1f109551bD432803012645Hac136c2Bb25F00c")?;
    
    database.create_account(deployer_address, U256::from(10_000_000_000_000_000_000u64)); // 10 ETH
    database.create_account(user_address, U256::from(5_000_000_000_000_000_000u64)); // 5 ETH

    let mut runtime = CatalystEvmRuntime::new(database, config);

    // 2. Simple contract bytecode (increments a counter)
    // This is a minimal contract that stores and increments a number
    let contract_bytecode = hex::decode(
        "608060405234801561001057600080fd5b5060008055348015602457600080fd5b50603f8060336000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c8063d09de08a14602d575b600080fd5b60336035565b005b60016000808282546046919060449061004a565b925050819055508056fea2646970667358221220a1b2c3d4e5f6708192a3b4c5d6e7f8901234567890abcdef1234567890abcdef64736f6c63430008070033"
    )?;

    // 3. Deploy the contract
    println!("📦 Deploying contract...");
    
    let deploy_tx = EvmTransaction {
        from: deployer_address,
        to: None, // Contract creation
        value: U256::zero(),
        data: contract_bytecode,
        gas_limit: 500_000,
        gas_price: U256::from(20_000_000_000u64), // 20 gwei
        gas_priority_fee: Some(U256::from(1_000_000_000u64)), // 1 gwei tip
        nonce: 0,
        chain_id: 31337,
        access_list: None,
    };

    let block_env = BlockEnv {
        number: U256::from(1),
        coinbase: Address::zero(),
        timestamp: U256::from(1640995200), // 2022-01-01
        gas_limit: U256::from(30_000_000),
        basefee: U256::from(1_000_000_000u64),
        difficulty: U256::zero(),
        prevrandao: Some(H256::random().into()),
    };

    let deploy_result = runtime.execute_transaction(&deploy_tx, &block_env).await?;

    if deploy_result.success() {
        println!("✅ Contract deployed successfully!");
        println!("   Gas used: {}", deploy_result.gas_used());
        
        if let Some(contract_address) = deploy_result.base_result.created_address {
            println!("   Contract address: {:?}", contract_address);
            
            // 4. Call the contract's increment function
            println!("📞 Calling contract increment function...");
            
            // Function signature for increment(): d09de08a
            let call_data = hex::decode("d09de08a")?;
            
            let call_tx = EvmTransaction {
                from: user_address,
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

            let call_result = runtime.execute_transaction(&call_tx, &block_env).await?;
            
            if call_result.success() {
                println!("✅ Contract call successful!");
                println!("   Gas used: {}", call_result.gas_used());
                println!("   Logs emitted: {}", call_result.logs().len());
            } else {
                println!("❌ Contract call failed");
            }
        }
    } else {
        println!("❌ Contract deployment failed");
        println!("   Return data: {:?}", deploy_result.return_data());
    }

    println!("🎉 Example completed!");
    Ok(())
}

// examples/cross_runtime_call.rs
//! Example demonstrating cross-runtime calls between EVM and other runtimes
//! 
//! This example shows:
//! - Calling from EVM to Solana runtime
//! - Handling cross-runtime data exchange
//! - Managing gas costs across runtimes

#[cfg(feature = "cross-runtime")]
use catalyst_runtime_evm::{
    CatalystEvmRuntime, CatalystEvmConfig, CatalystFeatures, EvmConfig,
    CrossRuntimeCall, RuntimeType
};

#[cfg(feature = "cross-runtime")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔗 Catalyst Cross-Runtime Call Example");
    
    // This would demonstrate calling between EVM and SVM runtimes
    // Implementation depends on the multi-runtime system being ready
    
    println!("Cross-runtime functionality coming soon!");
    Ok(())
}

#[cfg(not(feature = "cross-runtime"))]
fn main() {
    println!("This example requires the 'cross-runtime' feature to be enabled.");
    println!("Run with: cargo run --example cross_runtime_call --features cross-runtime");
}

// examples/gas_estimation.rs
//! Example demonstrating gas estimation and optimization

use catalyst_runtime_evm::{
    CatalystEvmRuntime, CatalystEvmConfig, CatalystFeatures, EvmConfig,
    EvmTransaction, FeeCalculator, GasMeter
};
use catalyst_runtime_evm::database::InMemoryDatabase;
use ethereum_types::{Address, U256};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("⛽ Gas Estimation Example");

    // 1. Setup runtime
    let config = CatalystEvmConfig {
        evm_config: EvmConfig::default(),
        catalyst_features: CatalystFeatures {
            catalyst_gas_model: true,
            ..Default::default()
        },
    };

    let database = InMemoryDatabase::new();
    let runtime = CatalystEvmRuntime::new(database, config);

    // 2. Gas estimation for different transaction types
    let transactions = vec![
        ("Simple Transfer", EvmTransaction {
            from: Address::random(),
            to: Some(Address::random()),
            value: U256::from(1_000_000_000_000_000_000u64), // 1 ETH
            data: vec![],
            gas_limit: 21000,
            gas_price: U256::from(20_000_000_000u64),
            gas_priority_fee: None,
            nonce: 0,
            chain_id: 31337,
            access_list: None,
        }),
        ("Contract Call", EvmTransaction {
            from: Address::random(),
            to: Some(Address::random()),
            value: U256::zero(),
            data: hex::decode("a9059cbb000000000000000000000000742d35cc6527c3c7bb91b6b4d0e71b46c7e3e8f4000000000000000000000000000000000000000000000000de0b6b3a7640000")?,
            gas_limit: 100000,
            gas_price: U256::from(20_000_000_000u64),
            gas_priority_fee: Some(U256::from(2_000_000_000u64)),
            nonce: 1,
            chain_id: 31337,
            access_list: None,
        }),
    ];

    // 3. Calculate gas and fees for each transaction
    let fee_calculator = FeeCalculator::new(U256::from(1_000_000_000u64));
    
    for (name, tx) in transactions {
        println!("\n📊 Transaction: {}", name);
        
        let mut gas_meter = GasMeter::new(tx.gas_limit, tx.gas_price);
        
        // Simulate some gas consumption
        let estimated_gas = match name {
            "Simple Transfer" => 21000,
            "Contract Call" => 45000,
            _ => 30000,
        };
        
        gas_meter.consume_gas(estimated_gas)?;
        
        let total_fee = fee_calculator.calculate_fee(
            gas_meter.gas_used(),
            tx.gas_priority_fee,
        );
        
        println!("   Gas Limit: {}", tx.gas_limit);
        println!("   Estimated Gas: {}", estimated_gas);
        println!("   Gas Remaining: {}", gas_meter.gas_remaining());
        println!("   Total Fee: {} wei", total_fee);
        println!("   Fee in ETH: {:.6}", total_fee.as_u128() as f64 / 1e18);
    }

    println!("\n✅ Gas estimation completed!");
    Ok(())
}