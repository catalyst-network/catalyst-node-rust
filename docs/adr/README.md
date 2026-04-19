# Architecture Decision Records (ADRs)

ADRs record **why** a protocol or system design choice was made, and **what** implementers should build so behavior stays consistent across crates and releases.

## Index

| ADR | Title | Status |
|-----|--------|--------|
| [0001](./0001-lsu-finality-certificate.md) | LSU finality certificate (cryptographic quorum evidence) | Proposed |

## Conventions

- **Numbering:** `NNNN-short-slug.md` (four digits, incrementing).
- **Status:** `Proposed` → `Accepted` → `Superseded` (with pointer to replacement ADR).
- **Implementation:** ADRs are normative for engineers; they complement [`../consensus-quorum-and-fork-choice.md`](../consensus-quorum-and-fork-choice.md) (requirements) and [`../implementation_backlog.md`](../implementation_backlog.md) (milestones).
