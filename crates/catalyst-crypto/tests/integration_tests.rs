//! Basic usage examples for catalyst-crypto

use catalyst_crypto::*;
use rand::thread_rng;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 Catalyst Crypto Examples\n");

    // Example 1: Basic key operations
    basic_key_operations()?;
    println!();

    // Example 2: Digital signatures
    digital_signatures()?;
    println!();

    // Example 3: MuSig aggregated signatures
    musig_signatures()?;
    println!();

    // Example 4: Hash operations
    hash_operations()?;
    println!();

    // Example 5: Confidential transactions
    confidential_transactions()?;

    Ok(())
}

fn basic_key_operations() -> CryptoResult<()> {
    println!("📋 Example 1: Basic Key Operations");
    
    let mut rng = thread_rng();
    
    // Generate a new key pair
    let keypair = KeyPair::generate(&mut rng);
    println!("✓ Generated new key pair");
    
    // Get the address
    let address = keypair.public_key().to_address();
    println!("  Address: {}", hex::encode(address));
    
    // Serialize and deserialize public key
    let pubkey_bytes = keypair.public_key().to_bytes();
    let recovered_pubkey = PublicKey::from_bytes(pubkey_bytes)?;
    println!("✓ Public key serialization works");
    
    // Verify they're the same
    assert_eq!(keypair.public_key().to_bytes(), recovered_pubkey.to_bytes());
    println!("✓ Key serialization roundtrip successful");
    
    Ok(())
}

fn digital_signatures() -> CryptoResult<()> {
    println!("📋 Example 2: Digital Signatures");
    
    let mut rng = thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let scheme = SignatureScheme::new();
    
    // Sign a message
    let message = b"Transfer 100 KAT to Alice";
    let signature = scheme.sign(&mut rng, keypair.private_key(), message)?;
    println!("✓ Created signature for message");
    
    // Verify the signature
    let is_valid = scheme.verify(message, &signature, keypair.public_key())?;
    println!("✓ Signature verification: {}", if is_valid { "VALID" } else { "INVALID" });
    
    // Test with wrong message
    let wrong_message = b"Transfer 200 KAT to Alice";
    let is_invalid = scheme.verify(wrong_message, &signature, keypair.public_key())?;
    println!("✓ Wrong message verification: {}", if is_invalid { "VALID" } else { "INVALID" });
    
    // Serialize signature
    let sig_bytes = signature.to_bytes();
    let recovered_sig = Signature::from_bytes(sig_bytes)?;
    let still_valid = scheme.verify(message, &recovered_sig, keypair.public_key())?;
    println!("✓ Serialized signature verification: {}", if still_valid { "VALID" } else { "INVALID" });
    
    Ok(())
}

fn musig_signatures() -> CryptoResult<()> {
    println!("📋 Example 3: MuSig Aggregated Signatures");
    
    let mut rng = thread_rng();
    let scheme = SignatureScheme::new();
    
    // Simulate a consensus scenario with 7 validators
    let validators: Vec<KeyPair> = (0..7).map(|_| KeyPair::generate(&mut rng)).collect();
    let consensus_message = b"Approve block #12345";
    
    println!("  Simulating consensus with {} validators", validators.len());
    
    // Each validator signs the consensus message
    let mut partial_signatures = Vec::new();
    for (i, validator) in validators.iter().enumerate() {
        let sig = scheme.sign(&mut rng, validator.private_key(), consensus_message)?;
        partial_signatures.push((sig, validator.public_key().clone()));
        println!("  ✓ Validator {} signed", i + 1);
    }
    
    // Aggregate all signatures
    let musig_signature = scheme.aggregate_signatures(consensus_message, &partial_signatures)?;
    println!("✓ Aggregated {} signatures into one", musig_signature.participant_count);
    
    // Verify the aggregated signature
    let consensus_valid = musig_signature.verify(consensus_message)?;
    println!("✓ Consensus signature verification: {}", if consensus_valid { "VALID" } else { "INVALID" });
    
    // Show space savings
    let individual_size = partial_signatures.len() * 64; // 64 bytes per signature
    let aggregated_size = 64; // Single signature
    println!("  Space savings: {} bytes → {} bytes ({:.1}% reduction)", 
             individual_size, aggregated_size, 
             (1.0 - aggregated_size as f64 / individual_size as f64) * 100.0);
    
    Ok(())
}

fn hash_operations() -> CryptoResult<()> {
    println!("📋 Example 4: Hash Operations");
    
    // Single hash
    let data = b"Catalyst Network - Fast, Secure, Scalable";
    let hash = blake2b_hash(data);
    println!("✓ Blake2b hash: {}", hash);
    
    // Multiple pieces
    let pieces = [&b"Block"[..], &b"Header"[..], &b"Data"[..]];
    let combined_hash = blake2b_hash_multiple(&pieces);
    println!("✓ Combined hash: {}", combined_hash);
    
    // Hash chain
    let mut current_hash = blake2b_hash(b"Genesis");
    for i in 1..=5 {
        let block_data = format!("Block {}", i);
        current_hash = blake2b_hash_multiple(&[current_hash.as_bytes(), block_data.as_bytes()]);
        println!("  Block {} hash: {}", i, current_hash);
    }
    
    // Verify deterministic
    let hash1 = blake2b_hash(data);
    let hash2 = blake2b_hash(data);
    assert_eq!(hash1, hash2);
    println!("✓ Hash function is deterministic");
    
    Ok(())
}

fn confidential_transactions() -> CryptoResult<()> {
    println!("📋 Example 5: Confidential Transactions");
    
    let mut rng = thread_rng();
    let commitment_scheme = CommitmentScheme::new();
    
    // Alice has 1000 KAT, wants to send 300 to Bob
    let alice_balance = 1000u64;
    let transfer_amount = 300u64;
    let alice_new_balance = alice_balance - transfer_amount;
    
    println!("  Alice balance: {} KAT", alice_balance);
    println!("  Transfer amount: {} KAT", transfer_amount);
    println!("  Alice new balance: {} KAT", alice_new_balance);
    
    // Create commitments (amounts are hidden)
    let alice_old_commit = commitment_scheme.commit(&mut rng, alice_balance);
    let transfer_commit = commitment_scheme.commit(&mut rng, transfer_amount);
    let alice_new_commit = commitment_scheme.commit(&mut rng, alice_new_balance);
    
    println!("✓ Created commitments (amounts are now hidden)");
    
    // Verify commitments (only Alice can do this, as she knows the values)
    assert!(commitment_scheme.verify(&alice_old_commit, alice_balance));
    assert!(commitment_scheme.verify(&transfer_commit, transfer_amount));
    assert!(commitment_scheme.verify(&alice_new_commit, alice_new_balance));
    println!("✓ Commitments verified by Alice");
    
    // Test homomorphic property: old_balance = new_balance + transfer
    let reconstructed = alice_new_commit.add(&transfer_commit);
    println!("✓ Demonstrated homomorphic addition");
    println!("  Original commitment: {}", hex::encode(alice_old_commit.to_bytes()));
    println!("  Reconstructed commitment: {}", hex::encode(reconstructed.to_bytes()));
    
    // Create range proof for transfer amount
    let range_proof = commitment_scheme.create_range_proof(
        &mut rng,
        &transfer_commit,
        transfer_amount,
        1,    // minimum: at least 1 KAT
        alice_balance  // maximum: can't exceed Alice's balance
    )?;
    
    println!("✓ Created range proof ({} bytes)", range_proof.len());
    
    // Verify range proof (anyone can do this)
    let proof_valid = commitment_scheme.verify_range_proof(
        &transfer_commit.commitment,
        &range_proof,
        1,
        alice_balance
    )?;
    
    println!("✓ Range proof verification: {}", if proof_valid { "VALID" } else { "INVALID" });
    println!("  ✓ Public can verify amount is in valid range without knowing exact value");
    
    Ok(())
}