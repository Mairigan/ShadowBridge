use anchor_lang::prelude::*;
use crate::{
    errors::TreasuryError,
    state::{Member, Proposal, ProposalStatus, Treasury, PROPOSAL_EXPIRY_SLOTS},
};

pub fn handler(ctx: Context<Propose>, params: ProposeParams) -> Result<()> {
    let treasury  = &mut ctx.accounts.treasury;
    let proposal  = &mut ctx.accounts.proposal;
    let clock     = Clock::get()?;

    require!(treasury.active, TreasuryError::NotActive);
    require!(
        params.amount <= treasury.max_per_proposal,
        TreasuryError::ExceedsCap,
    );

    proposal.treasury          = treasury.key();
    proposal.proposer          = ctx.accounts.proposer.key();
    proposal.message_hash      = params.message_hash;
    proposal.user_pubkey       = params.user_pubkey;
    proposal.signature_scheme  = params.signature_scheme;
    proposal.amount            = params.amount;
    proposal.yes_votes         = 0;
    proposal.no_votes          = 0;
    proposal.status            = ProposalStatus::Open;
    proposal.created_at        = clock.unix_timestamp;
    proposal.executed_at       = 0;
    proposal.expires_at_slot   = clock.slot
        .checked_add(PROPOSAL_EXPIRY_SLOTS)
        .ok_or(TreasuryError::Overflow)?;
    proposal.index             = treasury.proposal_count;
    // Store the MessageApproval bump so cast_vote can pass it to approve_message CPI
    proposal.message_approval_bump = ctx.bumps.message_approval;
    proposal.bump              = ctx.bumps.proposal;

    treasury.proposal_count = treasury.proposal_count
        .checked_add(1)
        .ok_or(TreasuryError::Overflow)?;

    emit!(ProposalCreated {
        proposal:        proposal.key(),
        treasury:        treasury.key(),
        proposer:        proposal.proposer,
        message_hash:    proposal.message_hash,
        amount:          proposal.amount,
        index:           proposal.index,
        expires_at_slot: proposal.expires_at_slot,
    });
    Ok(())
}

// ── Params ────────────────────────────────────────────────────────────────────

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ProposeParams {
    /// sha256 of the transaction bytes to be signed on the target chain
    pub message_hash:      [u8; 32],
    /// User pubkey required by Ika's approve_message
    pub user_pubkey:       [u8; 32],
    /// 0=Ed25519, 1=Secp256k1, 2=Secp256r1
    pub signature_scheme:  u8,
    /// Lamport amount being authorised
    pub amount:            u64,
}

// ── Accounts ──────────────────────────────────────────────────────────────────

#[derive(Accounts)]
#[instruction(params: ProposeParams)]
pub struct Propose<'info> {
    #[account(
        mut,
        seeds = [b"treasury", treasury.admin.as_ref()],
        bump  = treasury.bump,
    )]
    pub treasury: Account<'info, Treasury>,

    /// Verify the proposer is a registered member
    #[account(
        seeds = [b"member", treasury.key().as_ref(), proposer.key().as_ref()],
        bump  = member.bump,
        has_one = treasury,
    )]
    pub member: Account<'info, Member>,

    #[account(
        init,
        payer = proposer,
        space = 8 + Proposal::INIT_SPACE,
        seeds = [
            b"proposal",
            treasury.key().as_ref(),
            &treasury.proposal_count.to_le_bytes(),
        ],
        bump,
    )]
    pub proposal: Account<'info, Proposal>,

    /// CHECK: The MessageApproval PDA that the Ika dWallet program will
    /// initialise when approve_message is called in cast_vote.
    /// We derive and record its bump here at proposal creation time.
    #[account(
        seeds = [
            b"message_approval",
            proposal.key().as_ref(),
        ],
        bump,
        seeds::program = treasury.dwallet_program,
    )]
    pub message_approval: UncheckedAccount<'info>,

    #[account(mut)]
    pub proposer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// ── Events ────────────────────────────────────────────────────────────────────

#[event]
pub struct ProposalCreated {
    pub proposal:        Pubkey,
    pub treasury:        Pubkey,
    pub proposer:        Pubkey,
    pub message_hash:    [u8; 32],
    pub amount:          u64,
    pub index:           u64,
    pub expires_at_slot: u64,
}
