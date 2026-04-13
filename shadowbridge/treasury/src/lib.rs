//! shadowbridge_treasury — Ika dWallet-controlled multi-sig treasury
//!
//! Uses anchor-lang 1.0 (required by ika-dwallet-anchor).
//! Ika dWallet program on devnet: 87W54kGYFQ1rgWqMeu4XTPHWXWmXSQCcjm8vCTfiq1oY
//! Ika gRPC: https://pre-alpha-dev-1.ika.ika-network.net:443
//!
//! Setup (done once off-chain before calling init_treasury):
//!   1. Call the Ika gRPC service to create a dWallet.
//!   2. Derive this program's CPI authority PDA:
//!        Pubkey::find_program_address(&[b"__ika_cpi_authority"], &PROGRAM_ID)
//!   3. Call transfer_dwallet on the Ika program to transfer dWallet authority
//!      to the CPI authority PDA.
//!   4. Call init_treasury.

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

// Replace with real ID after: anchor deploy --provider.cluster devnet
declare_id!("TrsryXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");

#[program]
pub mod shadowbridge_treasury {
    use super::*;

    /// Initialise a new dWallet-controlled treasury.
    pub fn init_treasury(ctx: Context<InitTreasury>, params: InitParams) -> Result<()> {
        init_treasury::handler_init(ctx, params)
    }

    /// Add a member eligible to vote on disbursement proposals.
    pub fn add_member(ctx: Context<AddMember>) -> Result<()> {
        init_treasury::handler_add_member(ctx)
    }

    /// Create a disbursement proposal.
    ///
    /// `message_hash` should be sha256(target_chain_tx_bytes).
    /// E.g. for Bitcoin: sha256(psbt_bytes).
    pub fn propose(ctx: Context<Propose>, params: ProposeParams) -> Result<()> {
        propose::handler(ctx, params)
    }

    /// Cast a vote on a proposal.
    ///
    /// When yes_votes reaches the quorum threshold, this instruction
    /// automatically CPI-calls the Ika dWallet program's approve_message.
    /// The Ika 2PC-MPC network then asynchronously produces the signature
    /// and writes it to the MessageApproval account.
    ///
    /// `cpi_authority_bump`: the bump for [b"__ika_cpi_authority"] PDA.
    /// Derive off-chain once: Pubkey::find_program_address(
    ///     &[b"__ika_cpi_authority"], &PROGRAM_ID)
    pub fn cast_vote(
        ctx: Context<CastVote>,
        vote: bool,
        cpi_authority_bump: u8,
    ) -> Result<()> {
        vote::handler(ctx, vote, cpi_authority_bump)
    }

    /// Read the Ika signature from the MessageApproval account and emit
    /// a SignatureReady event for off-chain relayers to broadcast.
    pub fn relay_signature(ctx: Context<RelaySignature>) -> Result<()> {
        relay::handler(ctx)
    }
}
