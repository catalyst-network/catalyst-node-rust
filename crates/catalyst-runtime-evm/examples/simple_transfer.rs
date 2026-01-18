// examples/simple_transfer.rs
use catalyst_runtime_evm::{
    CatalystDatabase, EvmExecutor, ExecutionContext, EvmTransaction,
    types::EvmTransaction as EvmTx,
};
use alloy_primitives::{Address, U256};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("Catalyst EVM - Simple Transfer Example");
    println!("=====================================");

    // Create database with some initial accounts
    let mut db = CatalystDatabase::new();
    
    // Create sender and receiver addresses
    let sender = Address::from([1u8; 20]);
    let receiver = Address::from([2u8; 20]);
    
    // Give sender some initial balance
    db.inner_mut().create_account(sender, U256::from(1000000000000000000u64)); // 1 ETH
    db.inner_mut().create_account(receiver, U256::ZERO);
    
    println!("Initial balances:");
    println!("Sender:   {} wei", db.inner().get_account(&sender).unwrap().balance);
    println!("Receiver: {} wei", db.inner().get_account(&receiver).unwrap().balance);

    // Create execution context
    let context = ExecutionContext::new(31337);
    
    // Create executor
    let mut executor = EvmExecutor::new(db, context);
    
    // Create a transfer transaction
    let transfer_tx = EvmTransaction::transfer(
        sender,
        receiver,
        U256::from(500000000000000000u64), // 0.5 ETH
        0, // nonce
        U256::from(21000000000u64), // gas price: 21 gwei
    );

    println!("\nExecuting transfer transaction...");
    println!("Amount: 0.5 ETH");
    println!("Gas limit: {}", transfer_tx.gas_limit);
    println!("Gas price: {} wei", transfer_tx.gas_price);

    // Execute the transaction
    match executor.execute_transaction(&transfer_tx) {
        Ok(result) => {
            println!("\n✅ Transaction executed successfully!");
            println!("Gas used: {}", result.gas_used());
            println!("Success: {}", result.success());
            
            if result.success() {
                // Check final balances
                let sender_balance = executor.db().inner().get_account(&sender).unwrap().balance;
                let receiver_balance = executor.db().inner().get_account(&receiver).unwrap().balance;
                
                println!("\nFinal balances:");
                println!("Sender:   {} wei", sender_balance);
                println!("Receiver: {} wei", receiver_balance);
                
                // Calculate total gas cost
                let gas_cost = U256::from(result.gas_used()) * transfer_tx.gas_price;
                println!("Gas cost: {} wei", gas_cost);
                
                // Verify the transfer
                let expected_sender = U256::from(1000000000000000000u64) 
                    - U256::from(500000000000000000u64) 
                    - gas_cost;
                let expected_receiver = U256::from(500000000000000000u64);
                
                assert_eq!(sender_balance, expected_sender);
                assert_eq!(receiver_balance, expected_receiver);
                
                println!("✅ Balances verified correctly!");
            }
        }
        Err(e) => {
            println!("❌ Transaction failed: {:?}", e);
        }
    }

    println!("\nExample completed!");
    Ok(())
}