#!/usr/bin/env bun
/**
 * setup_treasury.ts
 *
 * One-time setup script:
 *   1. Creates a dWallet via Ika gRPC
 *   2. Transfers its authority to the treasury program's CPI authority PDA
 *   3. Calls init_treasury on-chain
 *   4. Registers members
 *   5. Writes treasury info to treasury.json for use by demo.ts
 *
 * Run: bun run src/bin/setup_treasury.ts
 *
 * Prerequisites:
 *   - treasury program deployed on devnet (TREASURY_PROGRAM_ID updated in treasury.ts)
 *   - WALLET_PATH set in .env (this wallet becomes the treasury admin)
 */

import { Connection, Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { readFileSync, writeFileSync } from "fs";
import { TreasuryClient, treasuryPda, TREASURY_PROGRAM_ID } from "../lib/treasury.js";
import { IkaService, SIG_SCHEME } from "../lib/ika.js";

const RPC          = process.env.SOLANA_RPC   ?? "https://api.devnet.solana.com";
const WALLET_PATH  = process.env.WALLET_PATH  ?? `${process.env.HOME}/.config/solana/id.json`;
const OUTPUT_FILE  = "treasury.json";

function loadKeypair(path: string): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(path, "utf8")) as number[])
  );
}

async function main() {
  const connection = new Connection(RPC, "confirmed");
  const admin      = loadKeypair(WALLET_PATH);

  const balance = await connection.getBalance(admin.publicKey);
  console.log(`Admin: ${admin.publicKey.toBase58()}`);
  console.log(`Balance: ${(balance / LAMPORTS_PER_SOL).toFixed(4)} SOL`);

  if (balance < 0.1 * LAMPORTS_PER_SOL) {
    console.error("Insufficient balance. Run: solana airdrop 2");
    process.exit(1);
  }

  // Generate member keypairs for the demo (3-of-5 quorum)
  // In production these would be real wallet pubkeys
  const members = Array.from({ length: 5 }, () => Keypair.generate());

  const client = new TreasuryClient(connection, admin);

  console.log("\n═══ ShadowBridge Treasury Setup ═══");
  const result = await client.setup(
    3,                                   // quorum: 3 of 5
    BigInt(100_000_000),                 // max 1 SOL per proposal
    SIG_SCHEME.Secp256k1                 // Bitcoin / Ethereum signing
  );

  console.log("\n═══ Adding Members ═══");
  for (const m of members) {
    await client.addMember(result.treasuryPubkey, m.publicKey);
  }

  // Persist for demo.ts
  const info = {
    treasuryPubkey:   result.treasuryPubkey.toBase58(),
    dwalletPubkey:    result.dwallet.pubkey.toBase58(),
    cpiAuthority:     result.cpiAuthority.toBase58(),
    cpiAuthorityBump: result.cpiAuthorityBump,
    quorum:           3,
    members:          members.map(m => ({
      pubkey:     m.publicKey.toBase58(),
      secretKey:  Array.from(m.secretKey),
    })),
  };

  writeFileSync(OUTPUT_FILE, JSON.stringify(info, null, 2));
  console.log(`\nTreasury info written to ${OUTPUT_FILE}`);
  console.log(`Treasury:   ${result.treasuryPubkey.toBase58()}`);
  console.log(`dWallet:    ${result.dwallet.pubkey.toBase58()}`);
  console.log(`Members:    ${members.length}`);
  console.log(`Quorum:     3 of 5`);
}

main().catch(err => { console.error(err); process.exit(1); });
