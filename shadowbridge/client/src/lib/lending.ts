/**
 * lending.ts
 *
 * TypeScript SDK for the shadowbridge_lending program (Anchor 0.32 / Encrypt).
 *
 * Full lifecycle for an encrypted lending position:
 *   1. createInputs()      — submit loan + collateral to Encrypt gRPC
 *   2. openPosition()      — register ciphertext Pubkeys on-chain
 *   3. runCollateralCheck() — call execute_graph (CollateralCheck)
 *   4. waitForExecutor()   — poll until output ciphertext is committed
 *   5. requestDecrypt()    — call request_decrypt on-chain
 *   6. waitForDecryptor()  — poll DecryptionResult account
 *   7. finalizeOpen()      — call finalize_open; position becomes Active
 */

import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import { EncryptService, ENCRYPT_PROGRAM_ID, type CiphertextRef } from "./encrypt.js";

// ── Constants ─────────────────────────────────────────────────────────────────

// Replace after: cd lending && anchor deploy --provider.cluster devnet
export const LENDING_PROGRAM_ID = new PublicKey(
  "LendXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
);

/** Max staleness for a collateral check: ~60 s at 400 ms/slot */
const MAX_CHECK_STALENESS_SLOTS = 150n;

// ── PDA helpers ───────────────────────────────────────────────────────────────

export function marketPda(collateralMint: PublicKey, borrowMint: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("market"), collateralMint.toBuffer(), borrowMint.toBuffer()],
    LENDING_PROGRAM_ID
  );
}

export function positionPda(
  market: PublicKey,
  borrower: PublicKey,
  index: bigint
): [PublicKey, number] {
  const idxBuf = Buffer.alloc(8);
  idxBuf.writeBigUInt64LE(index);
  return PublicKey.findProgramAddressSync(
    [Buffer.from("position"), market.toBuffer(), borrower.toBuffer(), idxBuf],
    LENDING_PROGRAM_ID
  );
}

export function graphRecordPda(position: PublicKey, slot: bigint): [PublicKey, number] {
  const slotBuf = Buffer.alloc(8);
  slotBuf.writeBigUInt64LE(slot);
  return PublicKey.findProgramAddressSync(
    [Buffer.from("graph"), position.toBuffer(), slotBuf],
    LENDING_PROGRAM_ID
  );
}

// ── GraphKind enum (mirrors Rust) ─────────────────────────────────────────────

export enum GraphKind {
  CollateralCheck  = 0,
  Repayment        = 1,
  LiquidationCheck = 2,
}

// ── LendingClient ─────────────────────────────────────────────────────────────

export interface OpenPositionResult {
  positionPubkey:    PublicKey;
  positionIndex:     bigint;
  loanCtPubkey:      PublicKey;
  collateralCtPubkey: PublicKey;
}

export class LendingClient {
  private connection: Connection;
  private payer:      Keypair;
  private encrypt:    EncryptService;

  constructor(connection: Connection, payer: Keypair) {
    this.connection = connection;
    this.payer      = payer;
    this.encrypt    = new EncryptService();
  }

  // ── Step 1: Encrypt inputs via Encrypt gRPC ─────────────────────────────

  /**
   * Encrypt loan + collateral amounts via Encrypt gRPC.
   * Returns two CiphertextRef objects whose pubkeys are passed to openPosition.
   */
  async createInputs(
    loanAmount: bigint,
    collateralAmount: bigint
  ): Promise<{ loanCt: CiphertextRef; collateralCt: CiphertextRef }> {
    console.log("  Encrypting inputs via Encrypt gRPC...");
    const [loanCt, collateralCt] = await this.encrypt.createInputPair(
      loanAmount,
      collateralAmount,
      LENDING_PROGRAM_ID
    );
    console.log(`    loan_ct:       ${loanCt.pubkey.toBase58()}`);
    console.log(`    collateral_ct: ${collateralCt.pubkey.toBase58()}`);
    return { loanCt, collateralCt };
  }

  // ── Step 2: open_position ───────────────────────────────────────────────

  async openPosition(
    marketPubkey: PublicKey,
    loanCt: CiphertextRef,
    collateralCt: CiphertextRef
  ): Promise<OpenPositionResult> {
    // Read market to get current position_count for PDA seed
    const marketInfo = await this.connection.getAccountInfo(marketPubkey);
    if (!marketInfo) throw new Error(`Market not found: ${marketPubkey.toBase58()}`);

    // Market layout (from state.rs):
    //   discriminator(8) + authority(32) + collateral_mint(32) + borrow_mint(32) +
    //   encrypt_program(32) + min_collateral_bps(2) + liquidation_bps(2) + position_count(8)
    const positionCount = marketInfo.data.readBigUInt64LE(8 + 32 + 32 + 32 + 32 + 2 + 2);

    const [positionPubkey] = positionPda(
      marketPubkey,
      this.payer.publicKey,
      positionCount
    );

    console.log(`  open_position → ${positionPubkey.toBase58()}`);

    // Build the open_position instruction using the Anchor 0.32 IDL client.
    // In production, use the generated IDL:
    //   import idl from "../../lending/target/idl/shadowbridge_lending.json";
    //   const program = new Program(idl, provider);
    //   const ix = await program.methods.openPosition().accounts({
    //     market:       marketPubkey,
    //     position:     positionPubkey,
    //     loanCt:       loanCt.pubkey,
    //     collateralCt: collateralCt.pubkey,
    //     borrower:     this.payer.publicKey,
    //     systemProgram: SystemProgram.programId,
    //   }).instruction();

    // Construct raw instruction matching the open_position account layout
    const ix = await this.buildIx("open_position", [
      { pubkey: marketPubkey,             isSigner: false, isWritable: true  },
      { pubkey: positionPubkey,           isSigner: false, isWritable: true  },
      { pubkey: loanCt.pubkey,            isSigner: false, isWritable: false },
      { pubkey: collateralCt.pubkey,      isSigner: false, isWritable: false },
      { pubkey: this.payer.publicKey,     isSigner: true,  isWritable: true  },
      { pubkey: SystemProgram.programId,  isSigner: false, isWritable: false },
    ]);

    const sig = await this.sendTx([ix]);
    console.log(`  Signature: ${sig}`);

    return {
      positionPubkey,
      positionIndex: positionCount,
      loanCtPubkey:  loanCt.pubkey,
      collateralCtPubkey: collateralCt.pubkey,
    };
  }

  // ── Step 3: execute_graph (CollateralCheck) ─────────────────────────────

  async runCollateralCheck(
    marketPubkey: PublicKey,
    positionPubkey: PublicKey,
    positionIndex: bigint
  ): Promise<{ outputCtPubkey: PublicKey; executedSlot: bigint }> {
    const slot = BigInt(await this.connection.getSlot());

    // The output_ct account will be initialised by the Encrypt program.
    // We pre-derive a keypair for it (Encrypt program will take ownership).
    const outputCtKeypair = Keypair.generate();

    const [graphRecord] = graphRecordPda(positionPubkey, slot);

    console.log(`  execute_graph(CollateralCheck) → output: ${outputCtKeypair.publicKey.toBase58()}`);

    const ix = await this.buildIx("execute_graph", [
      { pubkey: marketPubkey,             isSigner: false, isWritable: false },
      { pubkey: positionPubkey,           isSigner: false, isWritable: true  },
      { pubkey: graphRecord,              isSigner: false, isWritable: true  },
      { pubkey: outputCtKeypair.publicKey, isSigner: false, isWritable: true },
      // extra_ct: pass SystemProgram.programId for CollateralCheck (unused)
      { pubkey: SystemProgram.programId,  isSigner: false, isWritable: false },
      // encrypt_cpi accounts (EncryptCpi bundle — depends on encrypt-anchor implementation)
      { pubkey: ENCRYPT_PROGRAM_ID,       isSigner: false, isWritable: false },
      { pubkey: this.payer.publicKey,     isSigner: true,  isWritable: true  },
      { pubkey: SystemProgram.programId,  isSigner: false, isWritable: false },
    ], this.encodeGraphKind(GraphKind.CollateralCheck));

    const sig = await this.sendTx([ix]);
    console.log(`  Signature: ${sig}`);

    return { outputCtPubkey: outputCtKeypair.publicKey, executedSlot: slot };
  }

  // ── Step 4: poll until executor commits ────────────────────────────────

  async waitForExecutor(outputCtPubkey: PublicKey): Promise<void> {
    console.log("  Waiting for Encrypt executor to commit graph output...");
    await this.encrypt.waitForGraphCommit(outputCtPubkey, this.connection);
    console.log("  Graph output committed ✓");
  }

  // ── Step 5: request_decrypt ─────────────────────────────────────────────

  async requestDecrypt(
    marketPubkey: PublicKey,
    positionPubkey: PublicKey,
    positionIndex: bigint,
    outputCtPubkey: PublicKey
  ): Promise<PublicKey> {
    const decryptionRequestKeypair = Keypair.generate();
    const [progAuth] = PublicKey.findProgramAddressSync(
      [Buffer.from("prog_auth")],
      LENDING_PROGRAM_ID
    );

    console.log(`  request_decrypt → decryption request: ${decryptionRequestKeypair.publicKey.toBase58()}`);

    const ix = await this.buildIx("request_decrypt", [
      { pubkey: marketPubkey,                          isSigner: false, isWritable: false },
      { pubkey: positionPubkey,                        isSigner: false, isWritable: false },
      { pubkey: outputCtPubkey,                        isSigner: false, isWritable: false },
      { pubkey: decryptionRequestKeypair.publicKey,    isSigner: false, isWritable: true  },
      { pubkey: progAuth,                              isSigner: false, isWritable: false },
      { pubkey: ENCRYPT_PROGRAM_ID,                   isSigner: false, isWritable: false },
      { pubkey: this.payer.publicKey,                  isSigner: true,  isWritable: true  },
      { pubkey: SystemProgram.programId,               isSigner: false, isWritable: false },
    ]);

    const sig = await this.sendTx([ix]);
    console.log(`  Signature: ${sig}`);

    return decryptionRequestKeypair.publicKey;
  }

  // ── Step 6: poll until decryptor responds ──────────────────────────────

  async waitForDecryptor(decryptionRequestPubkey: PublicKey): Promise<boolean> {
    console.log("  Waiting for Encrypt decryptor to respond...");
    const value = await this.encrypt.waitForDecryption(
      decryptionRequestPubkey,
      this.connection
    );
    console.log(`  Decrypted value: ${value} (${value !== 0n ? "true" : "false"})`);
    return value !== 0n;
  }

  // ── Step 7: finalize_open ───────────────────────────────────────────────

  async finalizeOpen(
    marketPubkey: PublicKey,
    positionPubkey: PublicKey,
    decryptionResultPubkey: PublicKey
  ): Promise<void> {
    console.log("  finalize_open...");

    const ix = await this.buildIx("finalize_open", [
      { pubkey: marketPubkey,              isSigner: false, isWritable: false },
      { pubkey: positionPubkey,            isSigner: false, isWritable: true  },
      { pubkey: decryptionResultPubkey,    isSigner: false, isWritable: false },
      { pubkey: this.payer.publicKey,      isSigner: true,  isWritable: false },
    ]);

    const sig = await this.sendTx([ix]);
    console.log(`  Position activated ✓  sig: ${sig}`);
  }

  // ── Helpers ─────────────────────────────────────────────────────────────

  private encodeGraphKind(kind: GraphKind): Buffer {
    // Borsh-encode the GraphKind enum variant index (u8)
    return Buffer.from([kind]);
  }

  private async buildIx(
    _name: string,
    keys: { pubkey: PublicKey; isSigner: boolean; isWritable: boolean }[],
    data: Buffer = Buffer.alloc(0)
  ): Promise<TransactionInstruction> {
    // In production: use program.methods.<name>().accounts({...}).instruction()
    // Here we construct a raw instruction for demonstration
    return new TransactionInstruction({
      programId: LENDING_PROGRAM_ID,
      keys,
      data,
    });
  }

  private async sendTx(instructions: TransactionInstruction[]): Promise<string> {
    const tx = new Transaction();
    for (const ix of instructions) tx.add(ix);
    tx.feePayer = this.payer.publicKey;
    tx.recentBlockhash = (await this.connection.getLatestBlockhash()).blockhash;
    tx.sign(this.payer);
    return this.connection.sendRawTransaction(tx.serialize());
  }
}
