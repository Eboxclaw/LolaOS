# Key Management — CARVED OUT (no decisions here)

Status: **deliberately undecided** (ADR-014). This file exists so that no
slice silently invents a credential scheme.

Facts that constrain the future discussion:

- Hyperliquid authenticates with **EIP-712 wallet signatures**, not API
  keys; API wallets (agent keys) exist for scoped trading.
- v0.1 executes on **testnet only**, and even then only after S7; the
  first signed order requires a throwaway wallet whose loss is acceptable.
- The target runtime includes **Android** later — any scheme must reason
  about mobile secure storage (keystore) from the start.
- No component may read secrets until this document contains an accepted
  design; `config/venue.yaml` stays `credentials.mode: none`.

Questions the future discussion must answer:

1. API wallet vs device wallet key for signing, and scoping (trade-only,
   no withdrawal)?
2. Where do keys live per platform (laptop keychain, Android keystore)?
3. Who/what authorizes the transition testnet → mainnet (ties into the
   leverage ladder, ADR-008)?
4. Rotation, revocation and the kill switch's relationship to key access.
