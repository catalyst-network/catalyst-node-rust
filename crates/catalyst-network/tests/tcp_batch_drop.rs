//! Checklist §7.3: TCP batch-drop / relay scenarios using real `NetworkService` listeners.
//!
//! Run with the simple TCP stack (not libp2p gossipsub):  
//! `cargo test -p catalyst-network --no-default-features --features test-hooks --test tcp_batch_drop`

#![cfg(all(feature = "test-hooks", not(feature = "libp2p-full")))]

use catalyst_network::gossip_sim::GossipSimConfig;
use catalyst_network::tcp_test_mesh::{TcpTestMesh, wait_for_message_type};
use catalyst_utils::network::{MessageEnvelope, RoutingInfo};
use catalyst_utils::MessageType;

fn batch_envelope(payload: Vec<u8>) -> MessageEnvelope {
    MessageEnvelope::new(MessageType::TransactionBatch, "tcp".into(), None, payload)
        .with_routing_info(RoutingInfo::new(0))
}

#[tokio::test]
async fn tcp_line_topology_delivers_batch_to_immediate_neighbor() {
    let mesh = TcpTestMesh::line_topology_three(30480)
        .await
        .expect("mesh");
    let mut rx1 = mesh.peer(1).subscribe_events().await;
    let env = batch_envelope(vec![1, 2, 3]);
    mesh.peer(0)
        .broadcast_envelope(&env)
        .await
        .expect("broadcast");
    assert!(
        wait_for_message_type(&mut rx1, MessageType::TransactionBatch, 3_000).await,
        "node 1 should receive batch from node 0 over TCP"
    );
}

#[tokio::test]
async fn tcp_line_topology_does_not_deliver_batch_to_two_hop_without_relay() {
    let mesh = TcpTestMesh::line_topology_three(30490)
        .await
        .expect("mesh");
    let mut rx2 = mesh.peer(2).subscribe_events().await;
    let env = batch_envelope(vec![4, 5]);
    mesh.peer(0)
        .broadcast_envelope(&env)
        .await
        .expect("broadcast");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(
        !rx2.try_recv().is_ok(),
        "node 2 must not receive batch until node 1 relays (no direct link from 0)"
    );
    mesh.peer(1)
        .broadcast_envelope(&env)
        .await
        .expect("relay");
    assert!(
        wait_for_message_type(&mut rx2, MessageType::TransactionBatch, 3_000).await,
        "node 2 should receive batch after one-hop relay over TCP"
    );
}

#[tokio::test]
async fn tcp_mesh_inject_skips_transaction_batch_to_configured_dest() {
    let mesh = TcpTestMesh::line_with_selective_inject(
        30500,
        GossipSimConfig {
            skip_dest_message: Some((2, MessageType::TransactionBatch)),
            ..GossipSimConfig::default()
        },
    )
    .await
    .expect("mesh");
    let mut rx1 = mesh.peer(1).subscribe_events().await;
    let mut rx2 = mesh.peer(2).subscribe_events().await;
    let commit = MessageEnvelope::new(MessageType::TransactionRequest, "c".into(), None, vec![9])
        .with_routing_info(RoutingInfo::new(0));
    let batch = batch_envelope(vec![8, 7]);
    mesh.peer(0).broadcast_envelope(&commit).await.expect("commit");
    mesh.peer(0).broadcast_envelope(&batch).await.expect("batch");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(rx1.try_recv().is_ok() || wait_for_message_type(&mut rx1, MessageType::TransactionBatch, 2_000).await);
    assert!(
        !wait_for_message_type(&mut rx2, MessageType::TransactionBatch, 600).await,
        "dest 2 must not get TransactionBatch when skip filter is set"
    );
    assert!(
        wait_for_message_type(&mut rx2, MessageType::TransactionRequest, 2_000).await,
        "other message types still delivered"
    );
}
