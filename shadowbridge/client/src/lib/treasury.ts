/**
 * treasury.ts
 *
 * TypeScript SDK for the shadowbridge_treasury program (Anchor 1.0 / Ika).
 *
 * Flow:
 *   setup()       → createDWallet + transferAuthority + initTreasury + addMembers
 *   propose()     → create a disbursement proposal with a message_hash
 *   vote()        → cast votes; quorum fires approve_message CPI automatically
 *   awaitSig()    → poll MessageApproval for the 2PC-MPC signature
 *   relay()       → emit SignatureReady event for off-chain relayers
 */

import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import {
  IkaService,
  IKA_DWALLET_PROGRAM_ID,
  SIG_SCHEME,
  CPI_AUTHORITY_SEED,
  type DWalletInfo,
} from "./ika.js";

// ── Constants ─────────────────────────────────────────────────────────────────

// Replace after: cd treasury && anchor deploy --provider.cluster devnet
export const TREASURY_PROGRAM_ID = new PublicKey(
  "TrsryXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
);

// ── PDA helpers ───────────────────────────────────────────────────────────────

export function treasuryPda(admin: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("treasury"), admin.toBuffer()],
    TREASURY_PROGRAM_ID
  );
}

export function memberPda(treasury: PublicKey, member: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("member"), treasury.toBuffer(), member.toBuffer()],
    TREASURY_PROGRAM_ID
  );
}

export function proposalPda(treasury: PublicKey, index: bigint): [PublicKey, number] {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(index);
  return PublicKey.findProgramAddressSync(
    [Buffer.from("proposal"), treasury.toBuffer(), buf],
    TREASURY_PROGRAM_ID
  );
}

export function voteRecordPda(proposal: PublicKey, voter: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("vote"), proposal.toBuffer(), voter.toBuffer()],
    TREASURY_PROGRAM_ID
  );
}

// ── TreasuryClient ────────────────────────────────────────────────────────────

export interface SetupResult {
  treasuryPubkey:   PublicKey;
  dwallet:          DWalletInfo;
  cpiAuthority:     PublicKey;
  cpiAuthorityBump: number;
}

export interface ProposalResult {
  proposalPubkey:     PublicKey;
  proposalIndex:      bigint;
  messageApprovalPubkey: PublicKey;
}

export class TreasuryClient {
  private connection: Connection;
  private admin:      Keypair;
  private ika:        IkaService;

  constructor(connection: Connection, admin: Keypair) {
    this.connection = connection;
    this.admin      = admin;
    this.ika        = new IkaService(connection);
  }

  // ── Setup ────────────────────────────────────────────────────────────────

  /**
   * Full one-time treasury setup:
   *   1. Create dWallet via Ika gRPC
   *   2. Derive CPI authority PDA
   *   3. Transfer dWallet authority to CPI authority PDA
   *   4. Call init_treasury on Solana
   */
  async setup(
    quorum: number,
    maxPerProposal: bigint,
    signatureScheme: number = SIG_SCHEME.Secp256k1
  ): Promise<SetupResult> {
    console.log("\n[1/4] Creating dWallet via Ika gRPC...");
    const dwallet = await this.ika.createDWallet(this.admin, signatureScheme);

    console.log("\n[2/4] Deriving CPI authority PDA...");
    const [cpiAuthority, cpiAuthorityBump] = IkaService.cpiAuthorityPda(TREASURY_PROGRAM_ID);
    console.log(`  CPI authority: ${cpiAuthority.toBase58()}  bump=${cpiAuthorityBump}`);

    console.log("\n[3/4] Transferring dWallet authority to CPI authority PDA...");
    await this.ika.transferAuthority(dwallet.pubkey, cpiAuthority, this.admin);

    console.log("\n[4/4] Calling init_treasury...");
    const [treasuryPubkey] = treasuryPda(this.admin.publicKey);

    const initIx = await this.buildIx("init_treasury", [
      { pubkey: treasuryPubkey,          isSigner: false, isWritable: true  },
      { pubkey: this.admin.publicKey,    isSigner: true,  isWritable: true  },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ], this.encodeInitParams(dwallet.pubkey, quorum, maxPerProposal));

    const sig = await this.sendTx([initIx]);
    console.log(`  Treasury: ${treasuryPubkey.toBase58()}  sig: ${sig}`);

    return { treasuryPubkey, dwallet, cpiAuthority, cpiAuthorityBump };
  }

  /**
   * Register a voting member. Call once per member after setup().
   */
  async addMember(treasuryPubkey: PublicKey, memberPubkey: PublicKey): Promise<void> {
    const [memberAccount] = memberPda(treasuryPubkey, memberPubkey);
    const ix = await this.buildIx("add_member", [
      { pubkey: treasuryPubkey,          isSigner: false, isWritable: true  },
      { pubkey: memberAccount,           isSigner: false, isWritable: true  },
      { pubkey: memberPubkey,            isSigner: false, isWritable: false },
      { pubkey: this.admin.publicKey,    isSigner: true,  isWritable: true  },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ]);
    const sig = await this.sendTx([ix]);
    console.log(`  Member added: ${memberPubkey.toBase58().slice(0, 12)}…  sig: ${sig}`);
  }

  // ── Propose ──────────────────────────────────────────────────────────────

  /**
   * Create a disbursement proposal.
   *
   * @param treasuryPubkey     Treasury PDA
   * @param messageHash        sha256(target_chain_tx_bytes) — 32 bytes
   * @param userPubkey         32-byte user pubkey (Ika requirement)
   * @param amount             Lamports being authorised
   * @param signatureScheme    0=Ed25519, 1=Secp256k1, 2=Secp256r1
   */
  async propose(
    proposerKeypair: Keypair,
    treasuryPubkey: PublicKey,
    messageHash: Uint8Array,
    userPubkey: Uint8Array,
    amount: bigint,
    signatureScheme: number = SIG_SCHEME.Secp256k1
  ): Promise<ProposalResult> {
    if (messageHash.length !== 32) throw new Error("messageHash must be 32 bytes");
    if (userPubkey.length !== 32)  throw new Error("userPubkey must be 32 bytes");

    // Read treasury to get proposal_count for PDA seed
    const tInfo = await this.connection.getAccountInfo(treasuryPubkey);
    if (!tInfo) throw new Error(`Treasury not found: ${treasuryPubkey.toBase58()}`);

    // Treasury layout (from state.rs InitSpace order):
    //   discriminator(8) + admin(32) + dwallet(32) + dwallet_program(32) +
    //   quorum(4) + member_count(4) + proposal_count(8) + ...
    const proposalCount = tInfo.data.readBigUInt64LE(8 + 32 + 32 + 32 + 4 + 4);

    const [proposalPubkey] = proposalPda(treasuryPubkey, proposalCount);
    const [messageApprovalPubkey, msgApprovalBump] = IkaService.messageApprovalPda(proposalPubkey);
    const [memberAccount] = memberPda(treasuryPubkey, proposerKeypair.publicKey);

    console.log(`\n  Proposal #${proposalCount}: ${proposalPubkey.toBase58()}`);
    console.log(`  MessageApproval PDA: ${messageApprovalPubkey.toBase58()}`);

    const ix = await this.buildIx("propose", [
      { pubkey: treasuryPubkey,          isSigner: false, isWritable: true  },
      { pubkey: memberAccount,           isSigner: false, isWritable: false },
      { pubkey: proposalPubkey,          isSigner: false, isWritable: true  },
      { pubkey: messageApprovalPubkey,   isSigner: false, isWritable: false },
      { pubkey: proposerKeypair.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ], this.encodeProposeParams(messageHash, userPubkey, signatureScheme, amount));

    const sig = await this.sendTx([ix], [proposerKeypair]);
    console.log(`  sig: ${sig}`);

    return { proposalPubkey, proposalIndex: proposalCount, messageApprovalPubkey };
  }

  // ── Vote ─────────────────────────────────────────────────────────────────

  /**
   * Cast a vote. When yes_votes reaches quorum, the program automatically
   * CPI-calls approve_message and the Ika network begins signing.
   *
   * @param voterKeypair        The voter's keypair
   * @param treasuryPubkey      Treasury PDA
   * @param proposal            Proposal metadata returned by propose()
   * @param vote                true = yes, false = no
   * @param cpiAuthorityBump    From setup() — the bump for [b"__ika_cpi_authority"]
   * @param dwalletPubkey       The dWallet account from setup()
   */
  async vote(
    voterKeypair: Keypair,
    treasuryPubkey: PublicKey,
    proposal: ProposalResult,
    vote: boolean,
    cpiAuthorityBump: number,
    dwalletPubkey: PublicKey
  ): Promise<void> {
    const [memberAccount] = memberPda(treasuryPubkey, voterKeypair.publicKey);
    const [voteRecord]    = voteRecordPda(proposal.proposalPubkey, voterKeypair.publicKey);
    const [cpiAuthority]  = IkaService.cpiAuthorityPda(TREASURY_PROGRAM_ID);

    console.log(
      `  ${voterKeypair.publicKey.toBase58().slice(0, 8)}… votes ${vote ? "YES ✓" : "NO ✗"}`
    );

    const ix = await this.buildIx("cast_vote", [
      { pubkey: treasuryPubkey,              isSigner: false, isWritable: false },
      { pubkey: memberAccount,               isSigner: false, isWritable: false },
      { pubkey: proposal.proposalPubkey,     isSigner: false, isWritable: true  },
      { pubkey: voteRecord,                  isSigner: false, isWritable: true  },
      // Ika dWallet accounts (always required — Anchor validates upfront)
      { pubkey: dwalletPubkey,               isSigner: false, isWritable: false },
      { pubkey: proposal.messageApprovalPubkey, isSigner: false, isWritable: true },
      { pubkey: cpiAuthority,                isSigner: false, isWritable: false },
      { pubkey: TREASURY_PROGRAM_ID,         isSigner: false, isWritable: false }, // this_program (executable)
      { pubkey: IKA_DWALLET_PROGRAM_ID,      isSigner: false, isWritable: false },
      { pubkey: voterKeypair.publicKey,      isSigner: true,  isWritable: true  },
      { pubkey: voterKeypair.publicKey,      isSigner: true,  isWritable: true  }, // payer
      { pubkey: SystemProgram.programId,     isSigner: false, isWritable: false },
    ], this.encodeVoteParams(vote, cpiAuthorityBump));

    const sig = await this.sendTx([ix], [voterKeypair]);
    console.log(`    sig: ${sig}`);
  }

  // ── Await signature ───────────────────────────────────────────────────────

  async awaitSignature(messageApprovalPubkey: PublicKey) {
    return this.ika.waitForSignature(messageApprovalPubkey);
  }

  // ── Relay ─────────────────────────────────────────────────────────────────

  async relay(
    treasuryPubkey: PublicKey,
    proposal: ProposalResult
  ): Promise<void> {
    const ix = await this.buildIx("relay_signature", [
      { pubkey: treasuryPubkey,              isSigner: false, isWritable: false },
      { pubkey: proposal.proposalPubkey,     isSigner: false, isWritable: false },
      { pubkey: proposal.messageApprovalPubkey, isSigner: false, isWritable: false },
      { pubkey: this.admin.publicKey,        isSigner: true,  isWritable: false },
    ]);
    const sig = await this.sendTx([ix]);
    console.log(`  relay_signature → SignatureReady event emitted  sig: ${sig}`);
  }

  // ── Encoding helpers ──────────────────────────────────────────────────────

  private encodeInitParams(dwallet: PublicKey, quorum: number, maxPerProposal: bigint): Buffer {
    const buf = Buffer.alloc(32 + 32 + 4 + 8);
    dwallet.toBuffer().copy(buf, 0);
    IKA_DWALLET_PROGRAM_ID.toBuffer().copy(buf, 32);
    buf.writeUInt32LE(quorum, 64);
    buf.writeBigUInt64LE(maxPerProposal, 68);
    return buf;
  }

  private encodeProposeParams(
    messageHash: Uint8Array,
    userPubkey: Uint8Array,
    signatureScheme: number,
    amount: bigint
  ): Buffer {
    const buf = Buffer.alloc(32 + 32 + 1 + 8);
    Buffer.from(messageHash).copy(buf, 0);
    Buffer.from(userPubkey).copy(buf, 32);
    buf.writeUInt8(signatureScheme, 64);
    buf.writeBigUInt64LE(amount, 65);
    return buf;
  }

  private encodeVoteParams(vote: boolean, cpiAuthorityBump: number): Buffer {
    return Buffer.from([vote ? 1 : 0, cpiAuthorityBump]);
  }

  private async buildIx(
    _name: string,
    keys: { pubkey: PublicKey; isSigner: boolean; isWritable: boolean }[],
    data: Buffer = Buffer.alloc(0)
  ): Promise<TransactionInstruction> {
    return new TransactionInstruction({ programId: TREASURY_PROGRAM_ID, keys, data });
  }

  private async sendTx(instructions: TransactionInstruction[], signers: Keypair[] = []): Promise<string> {
    const tx = new Transaction();
    for (const ix of instructions) tx.add(ix);
    tx.feePayer = this.admin.publicKey;
    tx.recentBlockhash = (await this.connection.getLatestBlockhash()).blockhash;
    tx.sign(this.admin, ...signers);
    return this.connection.sendRawTransaction(tx.serialize());
  }
}
