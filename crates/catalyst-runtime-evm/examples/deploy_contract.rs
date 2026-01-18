// examples/deploy_contract.rs
use catalyst_runtime_evm::{
    CatalystDatabase, EvmExecutor, ExecutionContext, EvmTransaction,
    types::EvmTransaction as EvmTx,
    execution::utils,
};
use alloy_primitives::{Address, U256, Bytes};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("Catalyst EVM - Contract Deployment Example");
    println!("==========================================");

    // Create database with deployer account
    let mut db = CatalystDatabase::new().with_precompiles();
    
    // Create deployer address
    let deployer = Address::from([1u8; 20]);
    
    // Give deployer some initial balance
    db.inner_mut().create_account(deployer, U256::from(10000000000000000000u64)); // 10 ETH
    
    println!("Deployer balance: {} wei", db.inner().get_account(&deployer).unwrap().balance);

    // Create execution context
    let context = ExecutionContext::new(31337);
    
    // Create executor
    let mut executor = EvmExecutor::new(db, context);
    
    // Simple contract bytecode that stores a value and returns it
    // This is roughly equivalent to:
    // contract SimpleStorage {
    //     uint256 public value;
    //     constructor(uint256 _value) { value = _value; }
    //     function getValue() public view returns (uint256) { return value; }
    // }
    let contract_bytecode = Bytes::from(vec![
        // Constructor: store constructor argument in storage slot 0
        0x60, 0x00, // PUSH1 0x00 (storage slot)
        0x35,       // CALLDATALOAD (get constructor arg)
        0x55,       // SSTORE (store value)
        
        // Runtime code starts here
        0x60, 0x0a, // PUSH1 0x0a (length of runtime code)
        0x60, 0x0c, // PUSH1 0x0c (offset of runtime code)
        0x60, 0x00, // PUSH1 0x00 (destination offset)
        0x39,       // CODECOPY
        0x60, 0x0a, // PUSH1 0x0a (length)
        0x60, 0x00, // PUSH1 0x00 (offset)
        0xf3,       // RETURN
        
        // Runtime code: simple getter function
        0x60, 0x00, // PUSH1 0x00 (storage slot)
        0x54,       // SLOAD (load value)
        0x60, 0x00, // PUSH1 0x00 (memory offset)
        0x52,       // MSTORE (store in memory)
        0x60, 0x20, // PUSH1 0x20 (32 bytes)
        0x60, 0x00, // PUSH1 0x00 (memory offset)
        0xf3,       // RETURN
    ]);

    println!("\nContract bytecode length: {} bytes", contract_bytecode.len());

    // Calculate contract address
    let contract_address = utils::calculate_create_address(&deployer, 0);
    println!("Predicted contract address: {:?}", contract_address);

    // Create deployment transaction with constructor argument (value = 42)
    let mut deployment_data = contract_bytecode.clone();
    // Add constructor argument (42) as 32-byte value
    let constructor_arg = U256::from(42).to_be_bytes::<32>();
    deployment_data.extend_from_slice(&constructor_arg);

    let deploy_tx = EvmTransaction::deploy_contract(
        deployer,
        deployment_data,
        U256::ZERO, // no ETH sent to contract
        0, // nonce
        U256::from(20000000000u64), // gas price: 20 gwei
        1000000, // gas limit: 1M gas
    );

    println!("\nDeploying contract...");
    println!("Gas limit: {}", deploy_tx.gas_limit);
    println!("Gas price: {} wei", deploy_tx.gas_price);

    // Execute deployment
    match executor.execute_transaction(&deploy_tx) {
        Ok(result) => {
            println!("\n✅ Contract deployment executed!");
            println!("Gas used: {}", result.gas_used());
            println!("Success: {}", result.success());
            
            if result.success() {
                if let Some(created_address) = result.created_address {
                    println!("Contract deployed at: {:?}", created_address);
                    assert_eq!(created_address, contract_address);
                    
                    // Verify contract exists
                    if let Ok(is_contract) = utils::is_contract(executor.db_mut(), &created_address) {
                        println!("Contract verification: {}", if is_contract { "✅ Has code" } else { "❌ No code" });
                    }
                    
                    // Try to call the contract to get the stored value
                    println!("\nCalling contract to retrieve stored value...");
                    
                    let call = catalyst_runtime_evm::EvmCall {
                        from: deployer,
                        to: created_address,
                        value: None,
                        data: Some(Bytes::new()), // Empty calldata for simple getter
                        gas_limit: Some(100000),
                    };
                    
                    match executor.call(&call) {
                        Ok(call_result) => {
                            println!("Call result:");
                            println!("  Success: {}", call_result.success);
                            println!("  Gas used: {}", call_result.gas_used);
                            println!("  Return data length: {}", call_result.return_data.len());
                            
                            if call_result.success && call_result.return_data.len() == 32 {
                                let value = U256::from_be_slice(&call_result.return_data);
                                println!("  Stored value: {}", value);
                                assert_eq!(value, U256::from(42));
                                println!("✅ Contract returned expected value!");
                            }
                        }
                        Err(e) => {
                            println!("❌ Contract call failed: {:?}", e);
                        }
                    }
                } else {
                    println!("❌ No contract address returned");
                }
            } else {
                println!("❌ Deployment failed");
                if let Some(reason) = utils::get_revert_reason(&result) {
                    println!("Revert reason: {}", reason);
                }
            }
        }
        Err(e) => {
            println!("❌ Deployment transaction failed: {:?}", e);
        }
    }

    // Check final deployer balance
    let final_balance = executor.db().inner().get_account(&deployer).unwrap().balance;
    println!("\nDeployer final balance: {} wei", final_balance);
    
    let gas_cost = U256::from(executor.execute_transaction(&deploy_tx).unwrap().gas_used()) * deploy_tx.gas_price;
    println!("Total gas cost: {} wei", gas_cost);

    println!("\nExample completed!");
    Ok(())
}