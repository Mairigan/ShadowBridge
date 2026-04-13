use anchor_lang::prelude::*;
use crate::{errors::TreasuryError, state::{Member, Treasury}};

pub fn handler_init(ctx: Context<InitTreasury>, params: InitParams) -> Result<()> {
    require!(params.quorum >= 1, TreasuryError::BadQuorum);

    let t = &mut ctx.accounts.treasury;
    t.admin           = ctx.accounts.admin.key();
    t.dwallet         = params.dwallet;
    t.dwallet_program = params.dwallet_program;
    t.quorum          = params.quorum;
    t.member_count    = 0;
    t.proposal_count  = 0;
    t.max_per_proposal = params.max_per_proposal;
    t.active          = true;
    t.bump            = ctx.bumps.treasury;

    emit!(TreasuryCreated {
        treasury:          t.key(),
        admin:             t.admin,
        dwallet:           t.dwallet,
        quorum:            t.quorum,
        max_per_proposal:  t.max_per_proposal,
    });
    Ok(())
}

pub fn handler_add_member(ctx: Context<AddMember>) -> Result<()> {
    let treasury = &mut ctx.accounts.treasury;
    require!(
        treasury.admin == ctx.accounts.admin.key(),
        TreasuryError::NotAdmin,
    );

    let m = &mut ctx.accounts.member;
    m.treasury  = treasury.key();
    m.pubkey    = ctx.accounts.new_member.key();
    m.added_at  = Clock::get()?.unix_timestamp;
    m.bump      = ctx.bumps.member;

    treasury.member_count = treasury.member_count
        .checked_add(1)
        .ok_or(TreasuryError::Overflow)?;

    // Enforce quorum ≤ member_count now that count has changed
    require!(
        treasury.quorum <= treasury.member_count,
        TreasuryError::BadQuorum,
    );
    Ok(())
}

// ── Params ────────────────────────────────────────────────────────────────────

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitParams {
    /// The dWallet account (authority already transferred to CPI authority PDA)
    pub dwallet: Pubkey,
    /// Ika dWallet program ID on devnet
    pub dwallet_program: Pubkey,
    /// Required yes-votes for approval
    pub quorum: u32,
    /// Max lamports any single proposal may authorise
    pub max_per_proposal: u64,
}

// ── Account contexts ──────────────────────────────────────────────────────────

#[derive(Accounts)]
pub struct InitTreasury<'info> {
    #[account(
        init,
        payer  = admin,
        space  = 8 + Treasury::INIT_SPACE,  // Anchor v1: InitSpace derive
        seeds  = [b"treasury", admin.key().as_ref()],
        bump,
    )]
    pub treasury: Account<'info, Treasury>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AddMember<'info> {
    #[account(
        mut,
        seeds = [b"treasury", treasury.admin.as_ref()],
        bump  = treasury.bump,
    )]
    pub treasury: Account<'info, Treasury>,

    #[account(
        init,
        payer = admin,
        space = 8 + Member::INIT_SPACE,
        seeds = [b"member", treasury.key().as_ref(), new_member.key().as_ref()],
        bump,
    )]
    pub member: Account<'info, Member>,

    /// CHECK: The pubkey being added as a member — no signing required
    pub new_member: UncheckedAccount<'info>,   // Anchor v1: UncheckedAccount

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// ── Events ────────────────────────────────────────────────────────────────────

#[event]
pub struct TreasuryCreated {
    pub treasury:         Pubkey,
    pub admin:            Pubkey,
    pub dwallet:          Pubkey,
    pub quorum:           u32,
    pub max_per_proposal: u64,
}
