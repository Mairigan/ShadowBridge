//! state.rs — on-chain account layouts for the encrypted lending market.
//!
//! "Encrypted" fields store Pubkeys pointing to Encrypt-program-owned
//! ciphertext accounts. The actual FHE ciphertexts live there; this
//! program only stores the references.

use anchor_lang::prelude::*;

// ─── Market ───────────────────────────────────────────────────────────────────

/// Global lending market state.
#[account]
pub struct Market {
    /// Admin authority — can update params and pause the market
    pub authority: Pubkey,
    /// SPL mint for collateral (e.g. wBTC, wSOL)
    pub collateral_mint: Pubkey,
    /// SPL mint for the borrowed token (e.g. USDC)
    pub borrow_mint: Pubkey,
    /// Encrypt program ID — used to verify ciphertext account ownership
    /// Devnet: 4ebfzWdKnrnGseuQpezXdG8yCdHqwQ1SSBHD3bWArND8
    pub encrypt_program: Pubkey,
    /// Minimum collateral ratio in basis points (e.g. 15000 = 150%)
    pub min_collateral_bps: u16,
    /// Liquidation threshold in basis points (e.g. 11000 = 110%)
    pub liquidation_bps: u16,
    /// Total positions ever opened (used as sequential PDA seed)
    pub position_count: u64,
    /// Whether new positions can be opened
    pub active: bool,
    pub bump: u8,
}

impl Market {
    pub const LEN: usize = 8  // discriminator
        + 32   // authority
        + 32   // collateral_mint
        + 32   // borrow_mint
        + 32   // encrypt_program
        + 2    // min_collateral_bps
        + 2    // liquidation_bps
        + 8    // position_count
        + 1    // active
        + 1;   // bump
}

// ─── LendingPosition ──────────────────────────────────────────────────────────

/// A single encrypted lending position.
///
/// All financial values are stored as references to Encrypt ciphertext
/// accounts — Pubkeys pointing to accounts owned by the Encrypt program.
/// Nobody can read loan amounts, collateral values, or outstanding balances
/// from this account alone.
#[account]
pub struct LendingPosition {
    pub borrower: Pubkey,
    pub market: Pubkey,

    // ── Ciphertext account references ────────────────────────────────────
    // Pubkeys of Encrypt-owned accounts. Values submitted via gRPC before
    // open_position is called.

    /// EUint64 ciphertext: loan amount in borrow-token lamports
    pub loan_ct: Pubkey,
    /// EUint64 ciphertext: original collateral in collateral-token lamports
    pub collateral_ct: Pubkey,
    /// EUint64 ciphertext: current outstanding balance (updated by graphs)
    /// Zero (Pubkey::default) until the first disburse graph runs.
    pub outstanding_ct: Pubkey,

    // ── Latest graph output references ───────────────────────────────────
    /// Output of the most recent check_collateral graph (EBool ciphertext)
    pub collateral_check_output_ct: Pubkey,
    /// Slot when the collateral check graph was executed
    pub collateral_check_slot: u64,

    /// Output of the most recent is_liquidatable graph (EBool ciphertext)
    pub liquidation_check_output_ct: Pubkey,

    // ── Position metadata ────────────────────────────────────────────────
    pub opened_at: i64,
    pub status: PositionStatus,
    /// Sequential index (matches the market.position_count value at open time)
    pub index: u64,
    pub bump: u8,
}

impl LendingPosition {
    pub const LEN: usize = 8   // discriminator
        + 32   // borrower
        + 32   // market
        + 32   // loan_ct
        + 32   // collateral_ct
        + 32   // outstanding_ct
        + 32   // collateral_check_output_ct
        + 8    // collateral_check_slot
        + 32   // liquidation_check_output_ct
        + 8    // opened_at
        + 1    // status (enum tag)
        + 8    // index
        + 1;   // bump
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum PositionStatus {
    /// open_position called; collateral check graph not yet executed
    PendingCheck,
    /// check_collateral graph executed; waiting for decryption result
    PendingDecryption,
    /// Collateral verified; loan active
    Active,
    /// apply_repayment graph running
    Repaying,
    /// Fully repaid
    Closed,
    /// Liquidated
    Liquidated,
}

// ─── GraphRecord ──────────────────────────────────────────────────────────────

/// Tracks a single execute_graph CPI call for off-chain polling.
///
/// The TypeScript client monitors these records to know when to call
/// request_decryption or finalize instructions.
#[account]
pub struct GraphRecord {
    pub position: Pubkey,
    pub graph_kind: GraphKind,
    /// Output ciphertext account created by Encrypt
    pub output_ct: Pubkey,
    /// Slot when execute_graph was called
    pub executed_at_slot: u64,
    pub bump: u8,
}

impl GraphRecord {
    pub const LEN: usize = 8 + 32 + 1 + 32 + 8 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum GraphKind {
    CollateralCheck,
    Repayment,
    LiquidationCheck,
}
