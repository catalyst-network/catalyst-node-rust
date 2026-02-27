# Adversarial test plan (working)

This document enumerates adversarial scenarios and how to test them. Some scenarios are suitable for CI; others require WAN/chaos harnesses.

## CI-suitable (fast, deterministic)

- **Envelope wire rejection**:
  - unknown `PROTOCOL_VERSION` should be rejected cleanly
- **Rate budget behavior**:
  - per-peer/per-conn budgets enforce msg/sec and bytes/sec caps
- **Backoff/jitter bounding**:
  - dial backoff clamps to configured maximum
  - jitter stays within configured maximum
- **Hop/loop bounds**:
  - rebroadcast stops after `max_hops`
  - messages don’t loop forever once local id is visited

## Integration/WAN harness (slow, non-deterministic)

- **Eclipse attempt**:
  - isolate a victim by providing only attacker bootstrap peers
  - verify victim can regain honest peers when at least one honest seed exists
- **Sybil pressure**:
  - connect N peers from limited IP space and verify per-IP caps / peer scoring keeps diversity
- **Partition + heal**:
  - split validators into two groups for T seconds, then heal
  - verify nodes converge and “reliable join” repair does not corrupt state
- **DoS flood**:
  - send mixed size payloads at high QPS
  - verify bounded CPU/memory and steady cycle production

## Metrics to record (for #241/#206)

- peer count over time, `min_peers` satisfaction
- message drop counts (oversize / budget exceeded / decode fail / version mismatch)
- CPU, memory, open fds, disk growth rate
- cycle liveness (no-gap applied_cycle)

