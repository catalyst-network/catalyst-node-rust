# Testing strategy
- Unit: `*/src/*` + `*/tests/*`
- Property: `*/proptests/*` (proptest)
- Integration: `catalyst-node/tests/*` spin up in-proc node
- Conformance: `conformance/*` run via `cargo test -p conformance` (TBD)
- Chaos: docker-based network impairment scripts in `deploy/devnet`
