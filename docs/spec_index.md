### Catalyst spec index (authoritative references)

This repo’s source of truth is the published PDFs:
- **Consensus protocol (v1.2)**: [CatalystConsensusPaper.pdf](https://catalystnet.org/media/CatalystConsensusPaper.pdf)
- **Network overview**: [IntroductionToCatalystNetwork.pdf](https://catalystnet.org/media/IntroductionToCatalystNetwork.pdf)

Use this index when creating GitHub tickets so each ticket has an explicit **spec reference** (section/page).

## Consensus paper (v1.2) — key areas

- **Producer nodes selection**: §2.2.1
- **Transaction types**: §4.1
- **Transaction structure**: §4.2
- **Transaction entries**: §4.3
- **Transaction signature**: §4.4
- **Transaction validity**: §4.5
- **Consensus protocol overview**: §5.2
  - **Construction phase**: §5.2.1
  - **Campaigning phase**: §5.2.2
  - **Voting phase**: §5.2.3
  - **Synchronisation phase**: §5.2.4
- **DFS + producer output**: see §5.2.4 and security notes around LSU production in Ch.6
- **Security considerations**: Ch.6 (notably selection, LSU production, signature scheme)

## Network intro — key areas

Use this doc primarily for:
- node roles and operational expectations (bootstrapping, peer connectivity)
- lifecycle/behaviors expected of nodes (especially for public testnet runbooks)

