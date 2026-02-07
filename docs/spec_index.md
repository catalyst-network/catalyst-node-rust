### Catalyst spec index (authoritative references)

This repo’s source of truth is the published PDFs:
- **Consensus protocol (v1.2)**: [CatalystConsensusPaper.pdf](https://catalystnet.org/media/CatalystConsensusPaper.pdf)
- **Network overview**: [IntroductionToCatalystNetwork.pdf](https://catalystnet.org/media/IntroductionToCatalystNetwork.pdf)

Use this index when creating GitHub tickets so each ticket has an explicit **spec reference** (section/page).

## Spec pin (what we are implementing against)

- **Consensus**: “Catalyst Network Research: a new Consensus Protocol” — **Version 1.2** (“Currently under review”).  
  Source: [CatalystConsensusPaper.pdf](https://catalystnet.org/media/CatalystConsensusPaper.pdf)
- **Network overview**: “Introduction to Catalyst Network”.  
  Source: [IntroductionToCatalystNetwork.pdf](https://catalystnet.org/media/IntroductionToCatalystNetwork.pdf)

Ticket/PR reference format:

- `Consensus v1.2 §X.Y (p.NN–NN): <short requirement>`
- `Network intro §X.Y (p.NN–NN): <short requirement>`

## Consensus paper (v1.2) — key areas

- **Producer nodes selection**: §2.2.1 (p.17)
- **Transaction types**: §4.1 (p.26)
- **Transaction structure**: §4.2 (p.26)
- **Transaction entries**: §4.3 (p.27)
- **Transaction signature**: §4.4 (p.29)
- **Transaction validity**: §4.5 (p.32)
- **Consensus protocol overview**: §5.2 (p.37)
  - **Construction phase**: §5.2.1 (p.37)
  - **Campaigning phase**: §5.2.2 (p.39)
  - **Voting phase**: §5.2.3 (p.41)
  - **Synchronisation phase**: §5.2.4 (p.44)
- **DFS + producer output**: see §5.2.4 and security notes around LSU production in Ch.6
- **Security considerations**: Ch.6 (p.47) (notably selection, LSU production, signature scheme)

## Network intro — key areas

Use this doc primarily for:
- node roles and operational expectations (bootstrapping, peer connectivity)
- lifecycle/behaviors expected of nodes (especially for public testnet runbooks)

