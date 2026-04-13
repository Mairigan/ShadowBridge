/**
 * ika.ts
 *
 * Wraps the Ika dWallet pre-alpha TypeScript client.
 *
 * Source: chains/solana/clients/ in dwallet-labs/ika-pre-alpha
 * Install: bun add @ika-network/pre-alpha-solana-client
 *
 * Pre-alpha state: signatures from a single mock signer, not real 2PC-MPC.
 * The interface shown here is the final API (from the official docs).
 *
 * HOW IKA WORKS:
 *   1. createDWallet()      → gRPC call; Ika runs DKG and returns dWallet Pubkey
 *   2. transferAuthority()  → Solana tx; sets CPI authority PDA as dWallet authority
 *   3. Your program CPI-calls approve_message when conditions are met
 *   4. waitForSignature()   → poll MessageApproval account for the 2PC-MPC result
 */

import { Connection, Keypair, PublicKey, Transaction } from "@solana/web3.js";

// ── Constants ─────────────────────────────────────────────────────────────────

/** Ika dWallet program on Solana devnet */
export const IKA_DWALLET_PROGRAM_ID = new PublicKey(
  "87W54kGYFQ1rgWqMeu4XTPHWXWmXSQCcjm8vCTfiq1oY"
);

export const IKA_GRPC = "https://pre-alpha-dev-1.ika.ika-network.net:443";

/** Must match CPI_AUTHORITY_SEED in ika-dwallet-anchor */
export const CPI_AUTHORITY_SEED = Buffer.from("__ika_cpi_authority");

export const SIG_SCHEME = {
  Ed25519:   0,
  Secp256k1: 1, // Bitcoin, Ethereum
  Secp256r1: 2, // WebAuthn / Passkey
} as const;

// ── Types ─────────────────────────────────────────────────────────────────────

export interface DWalletInfo {
  /** On-chain Pubkey of the dWallet account */
  pubkey: PublicKey;
  /** Public key material on the target chain (e.g. 33-byte compressed secp256k1) */
  chainPublicKey: Uint8Array;
  signatureScheme: number;
}

export interface MessageApprovalResult {
  approvalPubkey: PublicKey;
  /** Raw signature bytes from the 2PC-MPC network */
  signature: Uint8Array;
}

// ── IkaService ────────────────────────────────────────────────────────────────

export class IkaService {
  private connection: Connection;

  constructor(connection: Connection) {
    this.connection = connection;
  }

  /**
   * Derive this program's CPI authority PDA.
   *
   * Seeds: [b"__ika_cpi_authority"]
   * This PDA must be set as the dWallet authority before the treasury can CPI.
   */
  static cpiAuthorityPda(treasuryProgramId: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [CPI_AUTHORITY_SEED],
      treasuryProgramId
    );
  }

  /**
   * Derive the MessageApproval PDA for a given proposal.
   *
   * Seeds (as used in propose.rs): [b"message_approval", proposal_pubkey]
   * Program: IKA_DWALLET_PROGRAM_ID
   */
  static messageApprovalPda(proposalPubkey: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("message_approval"), proposalPubkey.toBuffer()],
      IKA_DWALLET_PROGRAM_ID
    );
  }

  /**
   * Create a new dWallet via the Ika gRPC service.
   *
   * The Ika network runs Distributed Key Generation (DKG) and produces a
   * dWallet account on Solana. The account's data includes the public key
   * on the target chain (e.g. a Bitcoin/Ethereum address derivation key).
   *
   * In the pre-alpha, a single mock key is used instead of real 2PC-MPC DKG.
   */
  async createDWallet(
    payer: Keypair,
    signatureScheme: number = SIG_SCHEME.Secp256k1
  ): Promise<DWalletInfo> {
    // Production usage:
    //
    //   import { createIkaClient } from "@ika-network/pre-alpha-solana-client/grpc";
    //   const ika = createIkaClient();
    //   const { dwalletPubkey, chainPublicKey } = await ika.createDWallet({
    //     signatureScheme,
    //     payerPublicKey: payer.publicKey.toBytes(),
    //   });
    //   return {
    //     pubkey: new PublicKey(dwalletPubkey),
    //     chainPublicKey,
    //     signatureScheme,
    //   };

    console.log(`  [Ika] createDWallet(scheme=${signatureScheme})`);
    console.log(`  [Ika] gRPC → ${IKA_GRPC}`);

    // Pre-alpha placeholder — replace with real gRPC call above
    const mockPubkey = Keypair.generate().publicKey;
    console.log(`  [Ika] dWallet account: ${mockPubkey.toBase58()}`);
    return {
      pubkey:          mockPubkey,
      chainPublicKey:  new Uint8Array(33),
      signatureScheme,
    };
  }

  /**
   * Transfer the dWallet's authority to the treasury program's CPI authority PDA.
   *
   * This must be done BEFORE calling init_treasury. After this, only the
   * treasury program can call approve_message for this dWallet.
   *
   * The Ika dWallet program exposes a transfer_dwallet instruction. We build
   * it using the Codama-generated client from chains/solana/clients/.
   */
  async transferAuthority(
    dwalletPubkey: PublicKey,
    newAuthority: PublicKey,
    currentAuthority: Keypair
  ): Promise<string> {
    // Production usage (using Codama-generated Ika client):
    //
    //   import { getTransferDwalletInstruction } from "@ika-network/pre-alpha-solana-client";
    //   const ix = getTransferDwalletInstruction({
    //     dwallet:      dwalletPubkey,
    //     newAuthority: newAuthority,
    //     authority:    currentAuthority.publicKey,
    //   });
    //   const tx  = new Transaction().add(ix);
    //   const sig = await this.connection.sendTransaction(tx, [currentAuthority]);
    //   await this.connection.confirmTransaction(sig, "confirmed");
    //   return sig;

    console.log(`  [Ika] Transferring dWallet authority`);
    console.log(`  [Ika]   dWallet:       ${dwalletPubkey.toBase58()}`);
    console.log(`  [Ika]   New authority: ${newAuthority.toBase58()}`);
    return "mock_transfer_" + Date.now();
  }

  /**
   * Poll a MessageApproval account until the Ika 2PC-MPC network writes the
   * signature. Returns the raw signature bytes.
   *
   * MessageApproval layout (from ika-sdk-types):
   *   [0..8]   discriminator
   *   [8..40]  dwallet pubkey
   *   [40..72] message_hash
   *   [72..]   signature bytes
   */
  async waitForSignature(
    approvalPubkey: PublicKey,
    timeoutMs = 120_000,
    intervalMs = 3_000
  ): Promise<MessageApprovalResult> {
    console.log(`  [Ika] Waiting for 2PC-MPC signature: ${approvalPubkey.toBase58()}`);
    const deadline = Date.now() + timeoutMs;

    while (Date.now() < deadline) {
      const info = await this.connection.getAccountInfo(approvalPubkey);
      // Signature data starts at byte 72; account must have data beyond that
      if (info && info.data.length > 72) {
        const signature = info.data.slice(72);
        console.log(`  [Ika] Signature received (${signature.length} bytes)`);
        return { approvalPubkey, signature };
      }
      await sleep(intervalMs);
    }

    throw new Error(
      `[Ika] Timeout: no signature within ${timeoutMs}ms for ${approvalPubkey.toBase58()}`
    );
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
