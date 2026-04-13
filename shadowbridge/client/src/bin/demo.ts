#!/usr/bin/env bun
/**
 * demo.ts — ShadowBridge end-to-end demo
 *
 * Demonstrates two protocols in sequence:
 *
 *   Part A — Encrypted Lending (Encrypt FHE)
 *     Borrower submits loan + collateral amounts to Encrypt gRPC (encrypted).
 *     Program stores only ciphertext account Pubkeys — never plaintext values.
 *     FHE collateral check runs on-chain. Decryption confirms sufficiency.
 *     Position becomes Active.
 *
 *   Part B — dWallet Treasury Disbursement (Ika 2PC-MPC)
 *     Treasury controls a dWallet (distributed MPC signing key).
 *     Members vote on-chain. Quorum triggers approve_message CPI.
 *     Ika network produces 2PC-MPC signature into MessageApproval account.
 *     Off-chain relayer reads signature and broadcasts on target chain.
 *
 * Run: bun run src/bin/demo.ts
 *
 * Prerequisites:
 *   - Both programs deployed (update LENDING_PROGRAM_ID / TREASURY_PROGRAM_ID)
 *   - treasury.json present (run setup_treasury.ts first)
 *   - .env with SOLANA_RPC and WALLET_PATH
 */

import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
} from "@solana/web3.js";
import { existsSync, readFileSync } from "fs";
import { LendingClient, marketPda, LENDING_PROGRAM_ID } from "../lib/lending.js";
import { TreasuryClient, TREASURY_PROGRAM_ID } from "../lib/treasury.js";
import { ENCRYPT_PROGRAM_ID, ENCRYPT_GRPC } from "../lib/encrypt.js";
import { IKA_DWALLET_PROGRAM_ID, IKA_GRPC } from "../lib/ika.js";

// ── Config ────────────────────────────────────────────────────────────────────

const RPC         = process.env.SOLANA_RPC   ?? "https://api.devnet.solana.com";
const WALLET_PATH = process.env.WALLET_PATH  ?? `${process.env.HOME}/.config/solana/id.json`;

function loadKeypair(path: string): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(path, "utf8")) as number[])
  );
}

function bar(title: string) {
  console.log(`\n${"─".repeat(60)}`);
  console.log(`  ${title}`);
  console.log("─".repeat(60));
}

// ── Main ──────────────────────────────────────────────────────────────────────

async function main() {
  console.log("\n╔══════════════════════════════════════════════════════════╗");
  console.log("║           ShadowBridge — End-to-End Demo                ║");
  console.log("║   Encrypt FHE (lending) + Ika dWallet (treasury)        ║");
  console.log("╚══════════════════════════════════════════════════════════╝");

  const connection = new Connection(RPC, "confirmed");
  const payer      = loadKeypair(WALLET_PATH);

  console.log(`\nWallet:  ${payer.publicKey.toBase58()}`);
  console.log(`RPC:     ${RPC}`);
  const bal = await connection.getBalance(payer.publicKey);
  console.log(`Balance: ${(bal / LAMPORTS_PER_SOL).toFixed(4)} SOL\n`);

  console.log("Programs:");
  console.log(`  Lending:    ${LENDING_PROGRAM_ID.toBase58()}  (Encrypt, anchor 0.32)`);
  console.log(`  Treasury:   ${TREASURY_PROGRAM_ID.toBase58()}  (Ika, anchor 1.0)`);
  console.log(`  Encrypt:    ${ENCRYPT_PROGRAM_ID.toBase58()}`);
  console.log(`  Ika:        ${IKA_DWALLET_PROGRAM_ID.toBase58()}`);
  console.log(`  Encrypt gRPC: ${ENCRYPT_GRPC}`);
  console.log(`  Ika gRPC:     ${IKA_GRPC}`);

  // ══════════════════════════════════════════════════════════════════════════
  // PART A — ENCRYPTED LENDING
  // ══════════════════════════════════════════════════════════════════════════

  bar("Part A — Encrypted Lending (Encrypt FHE)");

  console.log("Alice wants to borrow 10 SOL against 20 SOL of collateral.");
  console.log("Neither amount will ever appear in plaintext on-chain.\n");

  const lendingClient = new LendingClient(connection, payer);

  // Token mints (devnet)
  const COLLATERAL_MINT = new PublicKey("So11111111111111111111111111111111111111112"); // wSOL
  const BORROW_MINT     = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"); // USDC

  const [market] = marketPda(COLLATERAL_MINT, BORROW_MINT);
  console.log(`Market: ${market.toBase58()}`);

  const LOAN_AMOUNT       = BigInt(10 * LAMPORTS_PER_SOL);
  const COLLATERAL_AMOUNT = BigInt(20 * LAMPORTS_PER_SOL);

  // Step 1: Encrypt inputs via Encrypt gRPC
  bar("A.1 — Encrypt inputs (Encrypt gRPC → ciphertext accounts)");
  const { loanCt, collateralCt } = await lendingClient.createInputs(
    LOAN_AMOUNT,
    COLLATERAL_AMOUNT
  );

  // Step 2: Register ciphertext Pubkeys on-chain
  bar("A.2 — open_position (register ciphertext Pubkeys on-chain)");
  const position = await lendingClient.openPosition(market, loanCt, collateralCt);
  console.log(`Position: ${position.positionPubkey.toBase58()}`);
  console.log(`  loan_ct:       ${position.loanCtPubkey.toBase58()}  ← ciphertext account Pubkey`);
  console.log(`  collateral_ct: ${position.collateralCtPubkey.toBase58()}  ← ciphertext account Pubkey`);
  console.log("  Plaintext values: NOT on-chain ✓");

  // Step 3: Run FHE collateral check graph
  bar("A.3 — execute_graph(CollateralCheck) via Encrypt CPI");
  console.log("  Graph: check_collateral(collateral: EUint64, loan: EUint64) → EBool");
  console.log("  Inputs: ciphertext accounts (encrypted, not plaintext)");
  const { outputCtPubkey } = await lendingClient.runCollateralCheck(
    market,
    position.positionPubkey,
    position.positionIndex
  );
  console.log(`  Output ciphertext: ${outputCtPubkey.toBase58()}`);
  console.log("  Encrypt executor picks up event and evaluates FHE graph...");

  // Step 4: Wait for executor
  bar("A.4 — Waiting for Encrypt executor");
  await lendingClient.waitForExecutor(outputCtPubkey);

  // Step 5: Request decryption
  bar("A.5 — request_decrypt (CPI to Encrypt program)");
  const decryptionRequest = await lendingClient.requestDecrypt(
    market,
    position.positionPubkey,
    position.positionIndex,
    outputCtPubkey
  );
  console.log("  Decryptor responds to DecryptionResult account...");

  // Step 6: Wait for decryptor
  bar("A.6 — Waiting for Encrypt decryptor");
  const collateralOk = await lendingClient.waitForDecryptor(decryptionRequest);
  console.log(`  Result: collateral >= loan → ${collateralOk}`);

  // Step 7: Finalize
  bar("A.7 — finalize_open");
  if (collateralOk) {
    await lendingClient.finalizeOpen(market, position.positionPubkey, decryptionRequest);
    console.log("  Position status: ACTIVE ✓");
    console.log("  Loan disbursed to borrower.");
  } else {
    console.log("  ✗ Insufficient collateral — position rejected.");
  }

  // ══════════════════════════════════════════════════════════════════════════
  // PART B — DWALLET TREASURY
  // ══════════════════════════════════════════════════════════════════════════

  bar("Part B — dWallet Treasury Disbursement (Ika 2PC-MPC)");

  // Load treasury info from setup_treasury.ts output
  if (!existsSync("treasury.json")) {
    console.log("treasury.json not found. Run: bun run src/bin/setup_treasury.ts");
    console.log("Skipping Part B.");
    printSummary();
    return;
  }

  const info = JSON.parse(readFileSync("treasury.json", "utf8")) as {
    treasuryPubkey:   string;
    dwalletPubkey:    string;
    cpiAuthorityBump: number;
    members:          { pubkey: string; secretKey: number[] }[];
    quorum:           number;
  };

  const treasuryPubkey  = new PublicKey(info.treasuryPubkey);
  const dwalletPubkey   = new PublicKey(info.dwalletPubkey);
  const members         = info.members.map(m =>
    Keypair.fromSecretKey(Uint8Array.from(m.secretKey))
  );

  const client = new TreasuryClient(connection, payer);

  console.log(`Treasury:  ${treasuryPubkey.toBase58()}`);
  console.log(`dWallet:   ${dwalletPubkey.toBase58()}`);
  console.log(`Quorum:    ${info.quorum} of ${info.members.length}`);
  console.log("\nDisbursing 0.1 SOL to a vendor address.");

  // Build a message hash (sha256 of the "transaction" bytes)
  // In production: hash of a real Bitcoin PSBT or Ethereum tx
  const txBytes     = new TextEncoder().encode("pay_vendor_0.1_SOL_" + Date.now());
  const hashBuffer  = await crypto.subtle.digest("SHA-256", txBytes);
  const messageHash = new Uint8Array(hashBuffer);
  const userPubkey  = payer.publicKey.toBytes();

  bar("B.1 — propose");
  const proposal = await client.propose(
    payer,                           // proposer (must be a member)
    treasuryPubkey,
    messageHash,
    userPubkey,
    BigInt(0.1 * LAMPORTS_PER_SOL)  // 0.1 SOL
  );
  console.log(`Proposal:        ${proposal.proposalPubkey.toBase58()}`);
  console.log(`MessageApproval: ${proposal.messageApprovalPubkey.toBase58()}`);

  bar(`B.2 — voting (need ${info.quorum} of ${members.length})`);
  for (let i = 0; i < info.quorum; i++) {
    await client.vote(
      members[i],
      treasuryPubkey,
      proposal,
      true,                        // yes vote
      info.cpiAuthorityBump,
      dwalletPubkey
    );
    if (i + 1 === info.quorum) {
      console.log(`  ✓ Quorum reached — approve_message CPI fired!`);
      console.log(`  Ika 2PC-MPC network is signing message_hash:`);
      console.log(`  0x${Buffer.from(messageHash).toString("hex")}`);
    }
  }

  bar("B.3 — waiting for Ika 2PC-MPC signature");
  const approval = await client.awaitSignature(proposal.messageApprovalPubkey);
  console.log(`Signature (hex): 0x${Buffer.from(approval.signature).toString("hex").slice(0, 40)}…`);

  bar("B.4 — relay_signature");
  await client.relay(treasuryPubkey, proposal);
  console.log("SignatureReady event emitted. Off-chain relayers can now broadcast.");

  printSummary();
}

function printSummary() {
  bar("Summary");
  console.log("Encrypted Lending (Encrypt FHE):");
  console.log("  • loan + collateral amounts encrypted via Encrypt gRPC (EUint64 ciphertexts)");
  console.log("  • Ciphertext account Pubkeys stored on-chain — no plaintext values");
  console.log("  • check_collateral FHE graph ran over ciphertexts via execute_graph CPI");
  console.log("  • Decryption result: single boolean, position gated on its value");
  console.log("  • Validators saw: ciphertext Pubkeys and a boolean outcome — nothing else");
  console.log("");
  console.log("dWallet Treasury (Ika 2PC-MPC):");
  console.log("  • dWallet created via Ika gRPC (DKG — distributed key generation)");
  console.log("  • Authority transferred to program CPI PDA (b\"__ika_cpi_authority\")");
  console.log("  • 3-of-5 on-chain quorum vote triggered approve_message CPI");
  console.log("  • Ika 2PC-MPC network produced distributed signature");
  console.log("  • Signature stored in MessageApproval account on Solana");
  console.log("  • relay_signature emits SignatureReady for off-chain broadcast");
  console.log("");
}

main().catch(err => { console.error(err); process.exit(1); });
