//! relay.rs — read the Ika signature from MessageApproval and emit it.
//!
//! After cast_vote fires the approve_message CPI and the Ika network produces
//! the 2PC-MPC signature, the MessageApproval account is populated.
//!
//! This instruction reads the raw signature bytes and emits a SignatureReady
//! event so off-chain relayers can broadcast the signed transaction on the
//! target chain (Bitcoin, Ethereum, etc.).
//!
//! MessageApproval account layout (from ika-sdk-types):
//!   [0..8]   discriminator     (8 bytes)
//!   [8..40]  dwallet pubkey    (32 bytes)
//!   [40..72] message_hash      (32 bytes)
//!   [72..]   signature bytes
//!            Secp256k1: 64 bytes (r ‖ s, no recovery byte)
//!            Ed25519:   64 bytes
//!            Secp256r1: 64 bytes (DER or raw — check Ika release notes)

use anchor_lang::prelude::*;
use crate::{
    errors::TreasuryError,
    state::{Proposal, ProposalStatus, Treasury},
};

pub fn handler(ctx: Context<RelaySignature>) -> Result<()> {
    let proposal = &ctx.accounts.proposal;

    require!(
        proposal.status == ProposalStatus::Approved,
        TreasuryError::AlreadyExecuted,
    );

    let data = ctx.accounts.message_approval.data.borrow();
    require!(data.len() >= 72, TreasuryError::BadApprovalData);

    // Verify the embedded message_hash matches our proposal
    let embedded: [u8; 32] = data[40..72].try_into().unwrap();
    require!(embedded == proposal.message_hash, TreasuryError::ApprovalHashMismatch);

    // Extract signature bytes (everything after byte 72)
    let signature: Vec<u8> = data[72..].to_vec();

    emit!(SignatureReady {
        proposal:        proposal.key(),
        treasury:        proposal.treasury,
        message_hash:    proposal.message_hash,
        signature_scheme: proposal.signature_scheme,
        amount:          proposal.amount,
        // The raw bytes — relay to target chain
        signature,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct RelaySignature<'info> {
    #[account(
        seeds = [b"treasury", treasury.admin.as_ref()],
        bump  = treasury.bump,
    )]
    pub treasury: Account<'info, Treasury>,

    #[account(
        has_one = treasury,
        seeds = [
            b"proposal",
            treasury.key().as_ref(),
            &proposal.index.to_le_bytes(),
        ],
        bump = proposal.bump,
    )]
    pub proposal: Account<'info, Proposal>,

    /// CHECK: MessageApproval account populated by the Ika 2PC-MPC network.
    /// We read raw bytes to extract the signature.
    pub message_approval: UncheckedAccount<'info>,

    pub caller: Signer<'info>,
}

#[event]
pub struct SignatureReady {
    pub proposal:         Pubkey,
    pub treasury:         Pubkey,
    pub message_hash:     [u8; 32],
    pub signature_scheme: u8,
    pub amount:           u64,
    /// Raw signature bytes — broadcast on target chain
    pub signature:        Vec<u8>,
}
