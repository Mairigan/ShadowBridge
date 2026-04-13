# ShadowBridge

**Two composable Solana programs — encrypted capital markets via [Encrypt](https://docs.encrypt.xyz) FHE and programmable MPC custody via [Ika dWallet](https://solana-pre-alpha.ika.xyz).**

---

## Overview

| Program | Infrastructure | What it does |
|---|---|---|
| `lending/` | **Encrypt** (anchor-lang 0.32) | Lending market where loan amounts and collateral values are FHE-encrypted on-chain. Validators never see plaintext. |
| `treasury/` | **Ika dWallet** (anchor-lang 1.0) | Multi-sig treasury that controls a distributed MPC signing key. Quorum vote on Solana triggers signing on any chain. |

> **Why two separate programs?** Encrypt requires `anchor-lang = "0.32"` and Ika requires `anchor-lang = "1"`. They cannot share a Cargo workspace with a single Anchor version. Each program has its own `Cargo.toml` and `Anchor.toml`.

---

## What each primitive actually does

### Encrypt (FHE)

You write `#[encrypt_fn]` Rust functions — they compile into FHE computation graphs. Your program calls `execute_graph` via CPI on-chain; the Encrypt executor evaluates the graph over real ciphertexts off-chain and commits the result. Decryption is requested separately and is asynchronous.

- **Program ID (devnet):** `4ebfzWdKnrnGseuQpezXdG8yCdHqwQ1SSBHD3bWArND8`
- **gRPC:** `https://pre-alpha-dev-1.encrypt.ika-network.net:443`
- **Anchor version:** 0.32

### Ika dWallet

Your Solana program controls a distributed MPC signing key. When your program's logic is satisfied (quorum vote, time-lock, etc.), it CPI-calls `approve_message`. The Ika validator network performs 2PC-MPC signing and writes the result to a `MessageApproval` account on Solana.

- **Program ID (devnet):** `87W54kGYFQ1rgWqMeu4XTPHWXWmXSQCcjm8vCTfiq1oY`
- **gRPC:** `https://pre-alpha-dev-1.ika.ika-network.net:443`
- **Anchor version:** 1.0.0

---

## Repository layout

```
shadowbridge/
├── lending/                     # Encrypt FHE lending (anchor 0.32)
│   ├── Cargo.toml
│   ├── Anchor.toml
│   ├── src/
│   │   ├── lib.rs               # Program entry + declare_id!
│   │   ├── errors.rs            # Single #[error_code] block
│   │   ├── state.rs             # Account structs
│   │   ├── fhe/
│   │   │   └── graphs.rs        # #[encrypt_fn] graph definitions
│   │   └── instructions/
│   │       ├── mod.rs
│   │       ├── init_market.rs
│   │       ├── open_position.rs
│   │       ├── execute_graph.rs
│   │       ├── request_decrypt.rs
│   │       └── finalize.rs
│   └── tests/
│       └── unit.rs
├── treasury/                    # Ika dWallet treasury (anchor 1.0)
│   ├── Cargo.toml
│   ├── Anchor.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── errors.rs
│   │   ├── state.rs
│   │   └── instructions/
│   │       ├── mod.rs
│   │       ├── init_treasury.rs
│   │       ├── add_member.rs
│   │       ├── propose.rs
│   │       ├── vote.rs          # CPI to approve_message on quorum
│   │       └── relay.rs
│   └── tests/
│       └── unit.rs
├── client/                      # TypeScript client (Bun)
│   ├── package.json
│   └── src/
│       ├── lib/
│       │   ├── encrypt.ts       # Encrypt gRPC wrapper
│       │   ├── ika.ts           # Ika gRPC wrapper
│       │   ├── lending.ts       # Lending SDK
│       │   └── treasury.ts      # Treasury SDK
│       └── bin/
│           └── demo.ts
├── scripts/
│   ├── deploy-lending.sh
│   ├── deploy-treasury.sh
│   └── setup-treasury.ts        # Create dWallet + transfer authority
├── docs/
│   ├── ENCRYPT.md
│   └── IKA.md
└── .github/workflows/ci.yml
```

---

## Prerequisites

```bash
# Rust nightly (required by both SDKs — edition 2024)
rustup toolchain install nightly
rustup default nightly

# Solana CLI 3.x
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"

# Anchor CLI — need BOTH versions
cargo install --git https://github.com/coral-xyz/anchor avm --force
avm install 0.32.0   # for lending (Encrypt)
avm install 1.0.0    # for treasury (Ika)

# Bun
curl -fsSL https://bun.sh/install | bash
```

---

## Build & deploy

```bash
# ── lending (Encrypt, anchor 0.32) ──────────────────────────────────────────
cd lending
avm use 0.32.0
anchor build
anchor deploy --provider.cluster devnet
# Copy program ID into lending/src/lib.rs declare_id! and lending/Anchor.toml

# ── treasury (Ika, anchor 1.0) ───────────────────────────────────────────────
cd ../treasury
avm use 1.0.0
anchor build
anchor deploy --provider.cluster devnet
# Copy program ID into treasury/src/lib.rs declare_id! and treasury/Anchor.toml
```

---

## Test

```bash
# Unit tests (no SBF needed)
cd lending  && cargo test
cd treasury && cargo test

# Integration tests (devnet)
cd lending  && anchor test --provider.cluster devnet
cd treasury && anchor test --provider.cluster devnet
```

---

## Run demo

```bash
cd client
bun install
cp ../.env.example .env
# Edit .env with your deployed program IDs

bun run src/bin/demo.ts
```

---

## Pre-alpha disclaimers

- **Encrypt:** No real FHE yet — values stored as plaintext. Interfaces are final; ciphertext semantics land in Alpha 1.
- **Ika:** No real MPC yet — single mock signer. Interfaces are final; 2PC-MPC lands in Alpha 1.
- All on-chain data wiped periodically.

---

## License

MIT
