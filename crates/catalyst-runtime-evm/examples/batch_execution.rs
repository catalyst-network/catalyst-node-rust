// examples/batch_execution.rs
use catalyst_runtime_evm::{
    CatalystDatabase, BatchExecutor, ExecutionContext, EvmTransaction,
};
use alloy_primitives::{Address, U256};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("Catalyst EVM - Batch Execution Example");
    println!("======================================");

    // Create database with multiple accounts
    let mut db = CatalystDatabase::new();
    
    // Create test addresses
    let addresses: Vec<Address> = (1..=5)
        .map(|i| Address::from([i; 20]))
        .collect();
    
    // Give each address some initial balance
    for (i, addr) in addresses.iter().enumerate() {
        let balance = U256::from((i + 1) * 1000000000000000000u64); // 1-5 ETH
        db.inner_mut().create_account(*addr, balance);
        println!("Account {}: {} wei", i + 1, balance);
    }

    // Create execution context
    let context = ExecutionContext::new(31337);
    
    // Create batch executor
    let mut batch_executor = BatchExecutor::new(db, context);
    
    // Create multiple transactions
    let mut transactions = Vec::new();
    
    // Transaction 1: Transfer from account 1 to account 2
    transactions.push(EvmTransaction::transfer(
        addresses[0],
        addresses[1],
        U256::from(500000000000000000u64), // 0.5 ETH
        0, // nonce
        U256::from(20000000000u64), // gas price: 20 gwei
    ));
    
    // Transaction 2: Transfer from account 2 to account 3
    transactions.push(EvmTransaction::transfer(
        addresses[1],
        addresses[2],
        U256::from(300000000000000000u64), // 0.3 ETH
        0, // nonce
        U256::from(20000000000u64), // gas price: 20 gwei
    ));
    
    // Transaction 3: Transfer from account 3 to account 4
    transactions.push(EvmTransaction::transfer(
        addresses[2],
        addresses[3],
        U256::from(200000000000000000u64), // 0.2 ETH
        0, // nonce
        U256::from(20000000000u64), // gas price: 20 gwei
    ));
    
    // Transaction 4: Transfer from account 4 to account 5
    transactions.push(EvmTransaction::transfer(
        addresses[3],
        addresses[4],
        U256::from(100000000000000000u64), // 0.1 ETH
        0, // nonce
        U256::from(20000000000u64), // gas price: 20 gwei
    ));

    println!("\nExecuting batch of {} transactions...", transactions.len());

    // Execute batch
    match batch_executor.execute_batch(&transactions) {
        Ok(results) => {
            println!("✅ Batch execution completed!");
            
            let mut total_gas_used = 0u64;
            let mut successful_txs = 0;
            
            for (i, result) in results.iter().enumerate() {
                println!("\nTransaction {}:", i + 1);
                println!("  Success: {}", result.success());
                println!("  Gas used: {}", result.gas_used());
                
                if result.success() {
                    successful_txs += 1;
                }
                total_gas_used += result.gas_used();
            }
            
            println!("\n📊 Batch Summary:");
            println!("  Total transactions: {}", transactions.len());
            println!("  Successful: {}", successful_txs);
            println!("  Failed: {}", transactions.len() - successful_txs);
            println!("  Total gas used: {}", total_gas_used);
            
            // Check final balances
            println!("\n💰 Final balances:");
            for (i, addr) in addresses.iter().enumerate() {
                if let Some(account) = batch_executor.executor().db().inner().get_account(addr) {
                    println!("  Account {}: {} wei", i + 1, account.balance);
                }
            }
            
            // Demonstrate moving to next block
            println!("\n🔗 Moving to next block...");
            let old_block = batch_executor.executor().context().block_number;
            batch_executor.next_block();
            let new_block = batch_executor.executor().context().block_number;
            println!("  Block number: {} -> {}", old_block, new_block);
            
        }
        Err(e) => {
            println!("❌ Batch execution failed: {:?}", e);
        }
    }

    println!("\nExample completed!");
    Ok(())
}