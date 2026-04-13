//! state.rs — on-chain account layouts for the dWallet treasury.
//!
//! Uses Anchor v1 patterns:
//!   - #[derive(InitSpace)] instead of manual LEN constants
//!   - UncheckedAccount instead of raw AccountInfo
//!   - Single #[error_code] block (enforced by Anchor v1)

use anchor_lang::prelude::*;

// ─── Treasury ─────────────────────────────────────────────────────────────────

/// A dWallet-controlled multi-signature treasury.
///
/// The treasury holds authority over a dWallet account managed by the Ika
/// network. Disbursements require a quorum of member votes. When quorum is
/// reached, the program CPI-calls approve_message on the Ika dWallet program,
/// causing the 2PC-MPC network to sign the proposal's message_hash.
///
/// The dWallet's authority must be transferred to this program's CPI authority
/// PDA ([b"__ika_cpi_authority"]) BEFORE calling init_treasury.
#[account]
#[derive(InitSpace)]
pub struct Treasury {
    /// Admin can add/remove members and update the quorum
    pub admin: Pubkey,
    /// The dWallet account whose authority is held by this program
    pub dwallet: Pubkey,
    /// Ika dWallet program ID: 87W54kGYFQ1rgWqMeu4XTPHWXWmXSQCcjm8vCTfiq1oY
    pub dwallet_program: Pubkey,
    /// Required number of yes-votes to approve a proposal
    pub quorum: u32,
    /// Total registered members
    pub member_count: u32,
    /// Total proposals ever created (used as sequential PDA seed)
    pub proposal_count: u64,
    /// Maximum lamports per proposal (a public spending cap)
    pub max_per_proposal: u64,
    pub active: bool,
    pub bump: u8,
}

// ─── Member ───────────────────────────────────────────────────────────────────

#[account]
#[derive(InitSpace)]
pub struct Member {
    pub treasury: Pubkey,
    pub pubkey: Pubkey,
    pub added_at: i64,
    pub bump: u8,
}

// ─── DisbursementProposal ─────────────────────────────────────────────────────

/// A proposal to disburse funds by having the dWallet sign a message.
///
/// The `message_hash` is a 32-byte hash of the transaction to sign on the
/// target chain (e.g. sha256 of a Bitcoin PSBT or Ethereum transaction).
///
/// When yes_votes >= treasury.quorum, cast_vote automatically CPI-calls
/// the Ika dWallet program's approve_message, and the Ika network produces
/// the 2PC-MPC signature into the MessageApproval account.
#[account]
#[derive(InitSpace)]
pub struct Proposal {
    pub treasury: Pubkey,
    pub proposer: Pubkey,
    /// 32-byte hash of the transaction to be signed by the dWallet
    pub message_hash: [u8; 32],
    /// 32-byte user public key (required by Ika's approve_message)
    pub user_pubkey: [u8; 32],
    /// Signature scheme: 0=Ed25519, 1=Secp256k1, 2=Secp256r1
    pub signature_scheme: u8,
    /// Lamport amount being authorised (checked against treasury.max_per_proposal)
    pub amount: u64,
    pub yes_votes: u32,
    pub no_votes: u32,
    pub status: ProposalStatus,
    pub created_at: i64,
    pub executed_at: i64,          // 0 if not yet executed
    pub expires_at_slot: u64,
    /// Sequential index (PDA seed)
    pub index: u64,
    /// MessageApproval PDA bump — stored here so cast_vote can pass it to CPI
    pub message_approval_bump: u8,
    pub bump: u8,
}

/// Slots until a proposal auto-expires (~2 days at 400ms/slot = 432_000 slots)
pub const PROPOSAL_EXPIRY_SLOTS: u64 = 432_000;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, InitSpace)]
pub enum ProposalStatus {
    Open,
    /// Quorum reached — approve_message CPI has been fired
    Approved,
    Rejected,
    Expired,
}

// ─── VoteRecord ───────────────────────────────────────────────────────────────

/// Records a single vote. PDA seeded by [b"vote", proposal, voter].
/// Anchor's `init` constraint makes double-voting a compile-time-enforced
/// protocol error — the account already exists on the second vote attempt.
#[account]
#[derive(InitSpace)]
pub struct VoteRecord {
    pub proposal: Pubkey,
    pub voter: Pubkey,
    pub vote: bool,
    pub voted_at: i64,
    pub bump: u8,
}
