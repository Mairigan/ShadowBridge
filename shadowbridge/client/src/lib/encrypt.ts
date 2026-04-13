/**
 * encrypt.ts
 *
 * Wraps @encrypt.xyz/pre-alpha-solana-client — the real TypeScript gRPC
 * client for the Encrypt pre-alpha network.
 *
 * Source: chains/solana/clients/ in dwallet-labs/encrypt-pre-alpha
 * Install: bun add @encrypt.xyz/pre-alpha-solana-client
 *
 * Pre-alpha state: no real FHE — values stored as plaintext.
 * The interface shown here is the final API (from the official docs).
 */

import {
  createEncryptClient,
  Chain,
  type EncryptClient as GrpcEncryptClient,
} from "@encrypt.xyz/pre-alpha-solana-client/grpc";
import { Connection, PublicKey } from "@solana/web3.js";

// ── Constants ─────────────────────────────────────────────────────────────────

/** Devnet Encrypt program — from https://docs.encrypt.xyz/getting-started/installation */
export const ENCRYPT_PROGRAM_ID = new PublicKey(
  "4ebfzWdKnrnGseuQpezXdG8yCdHqwQ1SSBHD3bWArND8"
);

/** gRPC endpoint (TLS) */
export const ENCRYPT_GRPC = "https://pre-alpha-dev-1.encrypt.ika-network.net:443";

/** FHE type tag for EUint64 */
const FHE_UINT64 = 4;

// ── EncryptService ────────────────────────────────────────────────────────────

export interface CiphertextRef {
  /** Pubkey of the Encrypt-owned on-chain ciphertext account */
  pubkey: PublicKey;
}

export class EncryptService {
  private grpc: GrpcEncryptClient;
  private networkKey: Uint8Array | null = null;

  constructor() {
    // createEncryptClient() uses ENCRYPT_GRPC_ENDPOINT with TLS automatically
    this.grpc = createEncryptClient();
  }

  /** Fetch the Encrypt network public key (cached after first call). */
  async networkEncryptionKey(): Promise<Uint8Array> {
    if (this.networkKey) return this.networkKey;
    const res = await this.grpc.getNetworkEncryptionKey({ chain: Chain.Solana });
    this.networkKey = res.networkEncryptionPublicKey;
    return this.networkKey;
  }

  /**
   * Encrypt a u64 value and register it as an on-chain ciphertext account.
   *
   * Returns a CiphertextRef whose `pubkey` is the on-chain address of an
   * Encrypt-program-owned account. Pass this pubkey to your Solana program
   * (e.g. as loan_ct or collateral_ct in open_position).
   *
   * @param value            The plaintext u64 to encrypt
   * @param authorizedProgram  Program allowed to use this ciphertext in execute_graph
   */
  async createInput(value: bigint, authorizedProgram: PublicKey): Promise<CiphertextRef> {
    const networkKey = await this.networkEncryptionKey();

    // Encode u64 as 8-byte little-endian
    const bytes = new Uint8Array(8);
    new DataView(bytes.buffer).setBigUint64(0, value, /* littleEndian */ true);

    const res = await this.grpc.createInput({
      chain: Chain.Solana,
      inputs: [{ ciphertextBytes: bytes, fheType: FHE_UINT64 }],
      authorized: authorizedProgram.toBytes(),
      networkEncryptionPublicKey: networkKey,
    });

    if (!res.ciphertextIdentifiers?.length) {
      throw new Error("Encrypt.createInput: no ciphertext identifier returned");
    }

    return { pubkey: new PublicKey(Buffer.from(res.ciphertextIdentifiers[0])) };
  }

  /**
   * Encrypt two u64 values in a single gRPC call (saves one round-trip).
   */
  async createInputPair(
    a: bigint,
    b: bigint,
    authorizedProgram: PublicKey
  ): Promise<[CiphertextRef, CiphertextRef]> {
    const networkKey = await this.networkEncryptionKey();

    const encode = (v: bigint): Uint8Array => {
      const buf = new Uint8Array(8);
      new DataView(buf.buffer).setBigUint64(0, v, true);
      return buf;
    };

    const res = await this.grpc.createInput({
      chain: Chain.Solana,
      inputs: [
        { ciphertextBytes: encode(a), fheType: FHE_UINT64 },
        { ciphertextBytes: encode(b), fheType: FHE_UINT64 },
      ],
      authorized: authorizedProgram.toBytes(),
      networkEncryptionPublicKey: networkKey,
    });

    if (!res.ciphertextIdentifiers || res.ciphertextIdentifiers.length < 2) {
      throw new Error("Encrypt.createInputPair: expected 2 ciphertext identifiers");
    }

    return [
      { pubkey: new PublicKey(Buffer.from(res.ciphertextIdentifiers[0])) },
      { pubkey: new PublicKey(Buffer.from(res.ciphertextIdentifiers[1])) },
    ];
  }

  /**
   * Poll an output ciphertext account until the Encrypt executor has committed
   * the result of an execute_graph call.
   *
   * The executor picks up the GraphExecutionRequested on-chain event, evaluates
   * the FHE graph, and calls commit_ciphertext. The account transitions from
   * empty (or just a discriminator) to populated with ciphertext data.
   *
   * @param outputCtPubkey   The output ciphertext account to poll
   * @param connection       Solana connection
   * @param timeoutMs        Max wait in milliseconds (default 60 s)
   * @param intervalMs       Poll interval in milliseconds (default 2 s)
   */
  async waitForGraphCommit(
    outputCtPubkey: PublicKey,
    connection: Connection,
    timeoutMs = 60_000,
    intervalMs = 2_000
  ): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const info = await connection.getAccountInfo(outputCtPubkey);
      // Account exists and has more than just the 8-byte discriminator
      if (info && info.data.length > 8) return;
      await sleep(intervalMs);
    }
    throw new Error(
      `Timeout: Encrypt executor did not commit output for ${outputCtPubkey.toBase58()}`
    );
  }

  /**
   * Poll a DecryptionResult account until the Encrypt decryptor has responded.
   *
   * Returns the raw u64 value (0 = false for EBool results, non-zero = true).
   *
   * DecryptionResult layout:
   *   [0..8]   discriminator
   *   [8]      type tag: 0=bool, 1=u64
   *   [9..17]  value as u64 LE
   */
  async waitForDecryption(
    decryptionResultPubkey: PublicKey,
    connection: Connection,
    timeoutMs = 60_000,
    intervalMs = 2_000
  ): Promise<bigint> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const info = await connection.getAccountInfo(decryptionResultPubkey);
      if (info && info.data.length >= 17) {
        const view = new DataView(info.data.buffer, info.data.byteOffset);
        return view.getBigUint64(9, /* littleEndian */ true);
      }
      await sleep(intervalMs);
    }
    throw new Error(
      `Timeout: Encrypt decryptor did not respond for ${decryptionResultPubkey.toBase58()}`
    );
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
