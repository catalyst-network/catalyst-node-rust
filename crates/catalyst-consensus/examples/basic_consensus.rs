use catalyst_consensus::{
    CatalystConsensus,
    ConsensusConfig,
    ConsensusMessage,
    ConsensusPhase, // Import the trait and types we need
    ConsensusProtocol,
    Transaction,
};
use catalyst_crypto::{KeyPair, Signature};
use catalyst_network::{NetworkConfig, NetworkService};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

async fn demonstrate_consensus_phases() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Catalyst Consensus Demo ===");

    // Create consensus configuration
    let config = ConsensusConfig {
        phase_duration_ms: 5000, // 5 seconds per phase
        min_producers: 3,
        max_producers: 100,
        message_timeout_ms: 2000,
    };

    println!("Consensus Configuration:");
    println!("  Phase Duration: {}ms", config.phase_duration_ms);
    println!("  Min Producers: {}", config.min_producers);
    println!("  Max Producers: {}", config.max_producers);
    println!("  Message Timeout: {}ms", config.message_timeout_ms);

    // Create network services for multiple nodes
    let mut nodes = Vec::new();
    let node_count = 3;

    for i in 0..node_count {
        let network_config = NetworkConfig::default();
        let network = Arc::new(NetworkService::new(network_config).await?);

        let mut rng = rand::thread_rng();
        let keypair = KeyPair::generate(&mut rng);
        let mut node_id = [0u8; 32];
        node_id[0] = i as u8; // Simple unique node IDs

        let consensus = CatalystConsensus::new(node_id, keypair, network);
        nodes.push((node_id, consensus));

        println!("Created node {} with ID: {:?}", i, node_id);
    }

    println!("\n=== Starting Consensus Simulation ===");

    // Simulate consensus phases with multiple nodes
    for (cycle, (node_id, consensus)) in nodes.iter_mut().enumerate() {
        println!("\n--- Node {:?} Consensus Cycle {} ---", node_id, cycle + 1);

        // Get initial state
        let state = consensus.get_state().await?;
        println!(
            "Initial state - Height: {}, Round: {}",
            state.current_height, state.current_round
        );

        // Create transactions for this cycle
        let transactions = create_test_transactions(cycle);
        println!("Created {} transactions for this cycle", transactions.len());

        // Phase 1: Construction - Propose block
        println!("Phase 1: Construction");
        let proposal = consensus.propose_block(transactions).await?;
        println!(
            "  Proposed block with {} transactions",
            proposal.transactions.len()
        );

        // Simulate phase duration
        sleep(Duration::from_millis(1000)).await;

        // Phase 2: Campaigning - Validate proposal
        println!("Phase 2: Campaigning");
        let is_valid = consensus.validate_proposal(&proposal).await?;
        println!("  Proposal validation: {}", is_valid);

        sleep(Duration::from_millis(1000)).await;

        // Phase 3: Voting - Vote on proposal
        println!("Phase 3: Voting");
        if is_valid {
            // Create a dummy signature for the demo
            use curve25519_dalek::{ristretto::RistrettoPoint, scalar::Scalar};

            let r = RistrettoPoint::default();
            let s = Scalar::ZERO;
            let signature = Signature::new(r, s);
            let vote_result = consensus.vote(&proposal.hash, signature).await;
            println!("  Vote result: {:?}", vote_result);
        }

        sleep(Duration::from_millis(1000)).await;

        // Phase 4: Synchronization - Get final state
        println!("Phase 4: Synchronization");
        let final_state = consensus.get_state().await?;
        println!(
            "  Final state - Height: {}, Votes: {}",
            final_state.current_height,
            final_state.votes.len()
        );

        sleep(Duration::from_millis(1000)).await;
    }

    println!("\n=== Consensus Demo Completed ===");
    Ok(())
}

fn create_test_transactions(cycle: usize) -> Vec<Transaction> {
    let base_id = (cycle * 100) as u8;
    vec![
        Transaction {
            id: [base_id; 32],
            data: format!("Transaction {} - Transfer KAT", base_id).into_bytes(),
        },
        Transaction {
            id: [base_id + 1; 32],
            data: format!("Transaction {} - Smart Contract Call", base_id + 1).into_bytes(),
        },
        Transaction {
            id: [base_id + 2; 32],
            data: format!("Transaction {} - Stake Tokens", base_id + 2).into_bytes(),
        },
    ]
}

fn demonstrate_consensus_messages() {
    println!("\n=== Consensus Message Types Demo ===");

    let node_id = [42u8; 32];
    let cycle_id = 1;

    // Construction Phase Message
    let quantity_msg = ConsensusMessage::ProducerQuantity {
        producer_id: node_id,
        hash_value: [1u8; 32],
        cycle_id,
    };
    println!("Construction Phase: {:?}", quantity_msg);

    // Campaigning Phase Message
    let candidate_msg = ConsensusMessage::ProducerCandidate {
        producer_id: node_id,
        candidate_hash: [2u8; 32],
        producer_list_hash: [3u8; 32],
        cycle_id,
    };
    println!("Campaigning Phase: {:?}", candidate_msg);

    // Voting Phase Message
    let vote_msg = ConsensusMessage::ProducerVote {
        producer_id: node_id,
        ledger_update_hash: [4u8; 32],
        voter_list_hash: [5u8; 32],
        cycle_id,
    };
    println!("Voting Phase: {:?}", vote_msg);

    // Synchronization Phase Message
    let output_msg = ConsensusMessage::ProducerOutput {
        producer_id: node_id,
        dfs_address: "QmHash123456789".to_string(),
        voter_list_hash: [6u8; 32],
        cycle_id,
    };
    println!("Synchronization Phase: {:?}", output_msg);
}

fn demonstrate_consensus_phases_enum() {
    println!("\n=== Consensus Phases Demo ===");

    let phases = vec![
        ConsensusPhase::Construction,
        ConsensusPhase::Campaigning,
        ConsensusPhase::Voting,
        ConsensusPhase::Synchronization,
    ];

    for (i, phase) in phases.iter().enumerate() {
        println!("Phase {}: {:?}", i + 1, phase);
        match phase {
            ConsensusPhase::Construction => {
                println!("  - Producers create partial ledger state updates");
                println!("  - Exchange producer quantities");
            }
            ConsensusPhase::Campaigning => {
                println!("  - Find majority partial update candidates");
                println!("  - Exchange producer candidates");
            }
            ConsensusPhase::Voting => {
                println!("  - Create final ledger state updates");
                println!("  - Exchange producer votes");
            }
            ConsensusPhase::Synchronization => {
                println!("  - Finalize and broadcast results");
                println!("  - Exchange producer outputs");
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Starting Basic Consensus Example");

    // Run the demonstrations
    demonstrate_consensus_phases_enum();
    demonstrate_consensus_messages();
    demonstrate_consensus_phases().await?;

    println!("\nBasic consensus example completed successfully!");
    Ok(())
}
