# ADR 0007: Freeze window + deterministic cycle input selection

## Status
Accepted (network-upgrade branch).

## Context
Catalyst consensus requires that all participating producers converge on the same Construction inputs for a given cycle, assuming they have the same mempool contents.

In practice, transactions may arrive near a cycle boundary. To avoid boundary jitter creating divergent Construction inputs, the protocol specifies a **freeze window** for Construction inputs (Consensus v1.2 §5.2.1).

Separately, even with identical mempool contents, nodes must apply a deterministic ordering and tie-break policy when selecting a bounded set of transactions for a cycle.

## Decision
For each cycle \(c\) with cycle duration \(T\) milliseconds:

- Define the cycle boundary timestamp: `cycle_start_ms = c * T` (epoch-aligned).
- Define a configured freeze window: `freeze_window_ms`.
- A transaction is **eligible** for inclusion in cycle \(c\) iff:
  - it is present in the node’s mempool, and
  - `received_at_ms <= cycle_start_ms - freeze_window_ms`.

Deterministic selection order:

- Sort candidate transactions by **canonical `tx_id` ascending** (bytewise).
- Apply deterministic validation/selection rules in that order (nonce sequencing, lock-time, sufficient funds, entry budget), producing the final cycle input set.

Implementation note (current system):

- The node currently uses a “batch leader” each cycle to gossip a `ProtocolTxBatch` so all producers use the same Construction entry list.
- This ADR does **not** remove leader batching yet; it makes the leader’s cycle input formation spec-shaped (freeze window + deterministic ordering) so that producers converge when their mempools match.

## Consequences
- Transactions arriving during the freeze window will be deferred to the next cycle.
- With identical mempool contents and consistent epoch alignment, producers will form the same cycle input set.
- The system remains compatible with the current leader-batching mechanism while moving toward a fully spec-shaped Construction input derivation.

