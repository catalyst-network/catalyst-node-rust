// benches/evm_execution.rs
//! Benchmarks for EVM execution performance

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use catalyst_runtime_evm::{
    CatalystEvmRuntime, CatalystEvmConfig, CatalystFeatures, EvmConfig,
    EvmTransaction, BlockEnv, GasMeter, FeeCalculator, ContractDeployer,
};
use catalyst_runtime_evm::database::InMemoryDatabase;
use ethereum_types::{Address, U256, H256};
use tokio::runtime::Runtime;

fn create_runtime() -> CatalystEvmRuntime<InMemoryDatabase> {
    let config = CatalystEvmConfig {
        evm_config: EvmConfig {
            chain_id: 31337,
            block_gas_limit: 30_000_000,
            base_fee: U256::from(1_000_000_000u64),
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
    
    // Setup test accounts
    let deployer = Address::from_low_u64_be(1);
    let user = Address::from_low_u64_be(2);
    
    database.create_account(deployer, U256::from(10_000_000_000_000_000_000u64));
    database.create_account(user, U256::from(10_000_000_000_000_000_000u64));

    CatalystEvmRuntime::new(database, config)
}

fn create_block_env() -> BlockEnv {
    BlockEnv {
        number: U256::from(1),
        coinbase: Address::zero(),
        timestamp: U256::from(1640995200),
        gas_limit: U256::from(30_000_000),
        basefee: U256::from(1_000_000_000u64),
        difficulty: U256::zero(),
        prevrandao: Some(H256::random().into()),
    }
}

fn benchmark_simple_transfer(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("simple_transfer", |b| {
        b.iter(|| {
            let mut runtime = create_runtime();
            let block_env = create_block_env();
            
            let tx = EvmTransaction {
                from: Address::from_low_u64_be(1),
                to: Some(Address::from_low_u64_be(2)),
                value: U256::from(1_000_000_000_000_000_000u64), // 1 ETH
                data: vec![],
                gas_limit: 21000,
                gas_price: U256::from(20_000_000_000u64),
                gas_priority_fee: None,
                nonce: 0,
                chain_id: 31337,
                access_list: None,
            };

            rt.block_on(async {
                black_box(runtime.execute_transaction(&tx, &block_env).await)
            })
        })
    });
}

fn benchmark_contract_deployment(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // Simple contract bytecode (storage example)
    let contract_bytecodes = vec![
        ("simple_storage", hex::decode("608060405234801561001057600080fd5b5060008055348015602457600080fd5b50603f8060336000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c8063d09de08a14602d575b600080fd5b60336035565b005b60016000808282546046919060449061004a565b925050819055508056fea264697066735822").unwrap()),
        ("erc20_token", vec![0x60, 0x80, 0x60, 0x40, 0x52, 0x34, 0x80, 0x15, 0x61, 0x00, 0x10, 0x57, 0x60, 0x00, 0x80, 0xfd]), // Simplified ERC20
    ];

    let mut group = c.benchmark_group("contract_deployment");
    
    for (name, bytecode) in contract_bytecodes {
        group.bench_with_input(BenchmarkId::new("deploy", name), &bytecode, |b, bytecode| {
            b.iter(|| {
                let mut runtime = create_runtime();
                let block_env = create_block_env();
                
                let tx = EvmTransaction {
                    from: Address::from_low_u64_be(1),
                    to: None, // Contract creation
                    value: U256::zero(),
                    data: bytecode.clone(),
                    gas_limit: 1_000_000,
                    gas_price: U256::from(20_000_000_000u64),
                    gas_priority_fee: None,
                    nonce: 0,
                    chain_id: 31337,
                    access_list: None,
                };

                rt.block_on(async {
                    black_box(runtime.execute_transaction(&tx, &block_env).await)
                })
            })
        });
    }
    group.finish();
}

fn benchmark_gas_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("gas_operations");
    
    group.bench_function("gas_meter_consume", |b| {
        b.iter(|| {
            let mut meter = GasMeter::new(1_000_000, U256::from(20_000_000_000u64));
            for i in 0..1000 {
                black_box(meter.consume_gas(i % 100 + 1));
            }
        })
    });

    group.bench_function("fee_calculation", |b| {
        let calculator = FeeCalculator::new(U256::from(1_000_000_000u64));
        b.iter(|| {
            for gas_used in [21000, 50000, 100000, 500000] {
                black_box(calculator.calculate_fee(
                    gas_used,
                    Some(U256::from(1_000_000_000u64))
                ));
            }
        })
    });

    group.finish();
}

fn benchmark_contract_address_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("address_calculation");
    
    group.bench_function("create_address", |b| {
        let mut deployer = ContractDeployer::new();
        let deployer_addr = Address::from_low_u64_be(1);
        let bytecode = vec![0x60, 0x60, 0x60, 0x40, 0x52];
        
        b.iter(|| {
            black_box(deployer.deploy_contract(
                deployer_addr,
                bytecode.clone(),
                vec![],
                None, // CREATE
            ))
        })
    });

    group.bench_function("create2_address", |b| {
        let mut deployer = ContractDeployer::new();
        let deployer_addr = Address::from_low_u64_be(1);
        let bytecode = vec![0x60, 0x60, 0x60, 0x40, 0x52];
        let salt = H256::from_low_u64_be(12345);
        
        b.iter(|| {
            black_box(deployer.deploy_contract(
                deployer_addr,
                bytecode.clone(),
                vec![],
                Some(salt), // CREATE2
            ))
        })
    });

    group.finish();
}

fn benchmark_transaction_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let transaction_counts = [10, 50, 100, 500];
    let mut group = c.benchmark_group("transaction_throughput");
    
    for &count in &transaction_counts {
        group.bench_with_input(BenchmarkId::new("batch", count), &count, |b, &count| {
            b.iter(|| {
                let mut runtime = create_runtime();
                let block_env = create_block_env();
                
                rt.block_on(async {
                    for i in 0..count {
                        let tx = EvmTransaction {
                            from: Address::from_low_u64_be(1),
                            to: Some(Address::from_low_u64_be(2)),
                            value: U256::from(1000 + i as u64),
                            data: vec![],
                            gas_limit: 21000,
                            gas_price: U256::from(20_000_000_000u64),
                            gas_priority_fee: None,
                            nonce: i as u64,
                            chain_id: 31337,
                            access_list: None,
                        };
                        
                        black_box(runtime.execute_transaction(&tx, &block_env).await);
                    }
                })
            })
        });
    }
    group.finish();
}

fn benchmark_precompiles(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("precompiles");
    
    // Benchmark standard precompiles
    let precompile_calls = vec![
        ("ecrecover", Address::from_low_u64_be(1), vec![0; 128]),
        ("sha256", Address::from_low_u64_be(2), b"hello world".to_vec()),
        ("ripemd160", Address::from_low_u64_be(3), b"hello world".to_vec()),
        ("identity", Address::from_low_u64_be(4), b"hello world".to_vec()),
    ];

    for (name, address, input_data) in precompile_calls {
        group.bench_with_input(BenchmarkId::new("call", name), &(address, input_data), |b, (addr, data)| {
            b.iter(|| {
                let mut runtime = create_runtime();
                let block_env = create_block_env();
                
                let tx = EvmTransaction {
                    from: Address::from_low_u64_be(1),
                    to: Some(*addr),
                    value: U256::zero(),
                    data: data.clone(),
                    gas_limit: 100_000,
                    gas_price: U256::from(20_000_000_000u64),
                    gas_priority_fee: None,
                    nonce: 0,
                    chain_id: 31337,
                    access_list: None,
                };

                rt.block_on(async {
                    black_box(runtime.execute_transaction(&tx, &block_env).await)
                })
            })
        });
    }
    
    group.finish();
}

criterion_group!(
    evm_benches,
    benchmark_simple_transfer,
    benchmark_contract_deployment,
    benchmark_gas_operations,
    benchmark_contract_address_calculation,
    benchmark_transaction_throughput,
    benchmark_precompiles
);

criterion_main!(evm_benches);