use catalyst_consensus::{
    CatalystConsensus, Transaction,
    ConsensusProtocol  // Import the trait to use its methods
};
use catalyst_network::{NetworkService, NetworkConfig};
use catalyst_crypto::{KeyPair, Signature};
use std::sync::Arc;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Starting Simple Consensus Example");
    
    // Create network configuration
    let network_config = NetworkConfig::default();
    
    // Create network service
    let network = Arc::new(NetworkService::new(network_config).await?);
    
    // Generate keypair for this node
    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32]; // Simple node ID for demo
    
    // Create consensus engine
    let mut consensus = CatalystConsensus::new(node_id, keypair, network);
    
    println!("Consensus engine created for node: {:?}", node_id);
    
    // Get initial state
    let initial_state = consensus.get_state().await?;
    println!("Initial consensus state:");
    println!("  Height: {}", initial_state.current_height);
    println!("  Round: {}", initial_state.current_round);
    println!("  Votes: {}", initial_state.votes.len());
    
    // Create some example transactions
    let transactions = vec![
        Transaction {
            id: [1u8; 32],
            data: b"Transfer 100 KAT from Alice to Bob".to_vec(),
        },
        Transaction {
            id: [2u8; 32],
            data: b"Transfer 50 KAT from Bob to Charlie".to_vec(),
        },
        Transaction {
            id: [3u8; 32],
            data: b"Deploy smart contract".to_vec(),
        },
    ];
    
    println!("Created {} test transactions", transactions.len());
    
    // Propose a block with these transactions
    let proposal = consensus.propose_block(transactions).await?;
    println!("Proposed block:");
    println!("  Height: {}", proposal.height);
    println!("  Timestamp: {}", proposal.timestamp);
    println!("  Transactions: {}", proposal.transactions.len());
    println!("  Producer: {:?}", proposal.producer_id);
    
    // Validate the proposal
    let is_valid = consensus.validate_proposal(&proposal).await?;
    println!("Proposal validation result: {}", is_valid);
    
    // Vote on the proposal (create a signature for demo)
    if is_valid {
        // Create a dummy signature for the demo
        // In a real implementation, this would be a proper signature from the keypair
        use curve25519_dalek::{ristretto::RistrettoPoint, scalar::Scalar};
        
        let r = RistrettoPoint::default();
        let s = Scalar::ZERO;
        let signature = Signature::new(r, s);
        
        let vote_result = consensus.vote(&proposal.hash, signature).await;
        println!("Vote result: {:?}", vote_result);
    }
    
    // Get final state
    let final_state = consensus.get_state().await?;
    println!("Final consensus state:");
    println!("  Height: {}", final_state.current_height);
    println!("  Round: {}", final_state.current_round);
    println!("  Votes: {}", final_state.votes.len());
    
    println!("Simple consensus example completed successfully!");
    
    Ok(())
}