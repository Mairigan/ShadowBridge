//! vote.rs — cast a vote; fire approve_message CPI on quorum.
//!
//! THE IKA CPI (from official docs and ika-pre-alpha repo):
//!
//!   use ika_dwallet_anchor::{DWalletContext, CPI_AUTHORITY_SEED};
//!
//!   // CPI_AUTHORITY_SEED = b"__ika_cpi_authority"
//!   // Derive once: Pubkey::find_program_address(&[CPI_AUTHORITY_SEED], &program_id)
//!
//!   let ctx = DWalletContext {
//!       dwallet_program:    <Ika program account info>,
//!       cpi_authority:      <your program's CPI authority PDA account info>,
//!       caller_program:     <your program's own account info (executable)>,
//!       cpi_authority_bump: <bump for the CPI authority PDA>,
//!   };
//!
//!   ctx.approve_message(
//!       &message_approval,   // MessageApproval PDA (Ika creates it)
//!       &dwallet,            // the dWallet account
//!       &payer,              // pays for MessageApproval rent
//!       &system_program,
//!       message_hash,        // [u8; 32]
//!       user_pubkey,         // [u8; 32]
//!       signature_scheme,    // u8
//!       bump,                // MessageApproval PDA bump
//!   )?;
//!
//! After this CPI, the Ika 2PC-MPC validator network picks up the on-chain
//! event and asynchronously produces a distributed signature, writing it
//! to the MessageApproval account.

use anchor_lang::prelude::*;
use ika_dwallet_anchor::{CPI_AUTHORITY_SEED, DWalletContext};
use crate::{
    errors::TreasuryError,
    state::{Member, Proposal, ProposalStatus, Treasury, VoteRecord},
};

pub fn handler(
    ctx: Context<CastVote>,
    vote: bool,
    cpi_authority_bump: u8,
) -> Result<()> {
    let treasury = &ctx.accounts.treasury;
    let proposal = &mut ctx.accounts.proposal;
    let record   = &mut ctx.accounts.vote_record;
    let clock    = Clock::get()?;

    // ── Validate proposal state ───────────────────────────────────────────
    require!(proposal.status == ProposalStatus::Open, TreasuryError::NotOpen);
    require!(clock.slot <= proposal.expires_at_slot,  TreasuryError::Expired);

    // ── Record vote ───────────────────────────────────────────────────────
    record.proposal  = proposal.key();
    record.voter     = ctx.accounts.voter.key();
    record.vote      = vote;
    record.voted_at  = clock.unix_timestamp;
    record.bump      = ctx.bumps.vote_record;

    if vote {
        proposal.yes_votes = proposal.yes_votes
            .checked_add(1).ok_or(TreasuryError::Overflow)?;
    } else {
        proposal.no_votes = proposal.no_votes
            .checked_add(1).ok_or(TreasuryError::Overflow)?;
    }

    emit!(VoteCast {
        proposal:  proposal.key(),
        voter:     ctx.accounts.voter.key(),
        vote,
        yes_votes: proposal.yes_votes,
        no_votes:  proposal.no_votes,
        quorum:    treasury.quorum,
    });

    // ── Check for rejection (can never reach quorum) ──────────────────────
    let votes_cast     = proposal.yes_votes + proposal.no_votes;
    let remaining      = treasury.member_count.saturating_sub(votes_cast);
    let max_possible   = proposal.yes_votes + remaining;
    if max_possible < treasury.quorum {
        proposal.status = ProposalStatus::Rejected;
        return Ok(());
    }

    // ── Check quorum ──────────────────────────────────────────────────────
    if proposal.yes_votes < treasury.quorum {
        return Ok(());
    }

    // ── Quorum reached — CPI to Ika dWallet program ───────────────────────
    //
    // Build DWalletContext using the real ika-dwallet-anchor SDK.
    // The cpi_authority PDA holds authority over the dWallet — it was set
    // as the dWallet's authority off-chain before treasury initialisation.

    let dwallet_ctx = DWalletContext {
        dwallet_program:    ctx.accounts.dwallet_program.to_account_info(),
        cpi_authority:      ctx.accounts.cpi_authority.to_account_info(),
        caller_program:     ctx.accounts.this_program.to_account_info(),
        cpi_authority_bump,
    };

    dwallet_ctx.approve_message(
        &ctx.accounts.message_approval.to_account_info(),
        &ctx.accounts.dwallet.to_account_info(),
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        proposal.message_hash,
        proposal.user_pubkey,
        proposal.signature_scheme,
        proposal.message_approval_bump,
    )?;

    // ── Update proposal ───────────────────────────────────────────────────
    proposal.status      = ProposalStatus::Approved;
    proposal.executed_at = clock.unix_timestamp;

    emit!(ProposalApproved {
        proposal:         proposal.key(),
        treasury:         treasury.key(),
        message_hash:     proposal.message_hash,
        message_approval: ctx.accounts.message_approval.key(),
        amount:           proposal.amount,
    });
    Ok(())
}

// ── Accounts ──────────────────────────────────────────────────────────────────

#[derive(Accounts)]
pub struct CastVote<'info> {
    #[account(
        seeds = [b"treasury", treasury.admin.as_ref()],
        bump  = treasury.bump,
    )]
    pub treasury: Account<'info, Treasury>,

    /// Verify voter is a registered treasury member
    #[account(
        seeds = [b"member", treasury.key().as_ref(), voter.key().as_ref()],
        bump  = member.bump,
        has_one = treasury,
    )]
    pub member: Account<'info, Member>,

    #[account(
        mut,
        has_one = treasury,
        seeds = [
            b"proposal",
            treasury.key().as_ref(),
            &proposal.index.to_le_bytes(),
        ],
        bump = proposal.bump,
    )]
    pub proposal: Account<'info, Proposal>,

    /// VoteRecord PDA — Anchor's `init` prevents double-voting by erroring
    /// if this account already exists (it would for a second vote from same voter)
    #[account(
        init,
        payer = voter,
        space = 8 + VoteRecord::INIT_SPACE,
        seeds = [b"vote", proposal.key().as_ref(), voter.key().as_ref()],
        bump,
    )]
    pub vote_record: Account<'info, VoteRecord>,

    // ── Ika dWallet accounts (required even before quorum — Anchor validates
    // ── all accounts upfront, so we always need them in the transaction) ──

    /// CHECK: The dWallet account — validated against treasury.dwallet
    #[account(address = treasury.dwallet @ TreasuryError::DWalletMismatch)]
    pub dwallet: UncheckedAccount<'info>,

    /// CHECK: MessageApproval PDA — the Ika program creates this during
    /// approve_message. Its seeds are owned by the Ika dWallet program.
    #[account(mut)]
    pub message_approval: UncheckedAccount<'info>,

    /// CHECK: This program's CPI authority PDA.
    /// Seeds: [CPI_AUTHORITY_SEED] = [b"__ika_cpi_authority"]
    /// This PDA holds authority over the dWallet (transferred off-chain).
    #[account(seeds = [CPI_AUTHORITY_SEED], bump)]
    pub cpi_authority: UncheckedAccount<'info>,

    /// CHECK: This program's own account. Must be marked executable.
    /// Required by DWalletContext to prove the CPI caller is a real program.
    #[account(executable)]
    pub this_program: UncheckedAccount<'info>,

    /// CHECK: Ika dWallet program — verified against treasury.dwallet_program
    #[account(address = treasury.dwallet_program)]
    pub dwallet_program: UncheckedAccount<'info>,

    #[account(mut)]
    pub voter: Signer<'info>,

    /// Payer for the MessageApproval account rent (may be same as voter)
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// ── Events ────────────────────────────────────────────────────────────────────

#[event]
pub struct VoteCast {
    pub proposal:  Pubkey,
    pub voter:     Pubkey,
    pub vote:      bool,
    pub yes_votes: u32,
    pub no_votes:  u32,
    pub quorum:    u32,
}

#[event]
pub struct ProposalApproved {
    pub proposal:         Pubkey,
    pub treasury:         Pubkey,
    pub message_hash:     [u8; 32],
    pub message_approval: Pubkey,
    pub amount:           u64,
}
