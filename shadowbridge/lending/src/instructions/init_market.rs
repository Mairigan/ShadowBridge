use anchor_lang::prelude::*;
use crate::{errors::LendingError, state::Market};

pub fn handler(ctx: Context<InitMarket>, params: InitMarketParams) -> Result<()> {
    require!(params.min_collateral_bps >= 10_000, LendingError::Unauthorized);
    require!(
        params.liquidation_bps >= 10_000
            && params.liquidation_bps < params.min_collateral_bps,
        LendingError::Unauthorized
    );

    let m = &mut ctx.accounts.market;
    m.authority          = ctx.accounts.authority.key();
    m.collateral_mint    = ctx.accounts.collateral_mint.key();
    m.borrow_mint        = ctx.accounts.borrow_mint.key();
    m.encrypt_program    = params.encrypt_program;
    m.min_collateral_bps = params.min_collateral_bps;
    m.liquidation_bps    = params.liquidation_bps;
    m.position_count     = 0;
    m.active             = true;
    m.bump               = ctx.bumps.market;

    emit!(MarketCreated {
        market:           m.key(),
        collateral_mint:  m.collateral_mint,
        borrow_mint:      m.borrow_mint,
        encrypt_program:  m.encrypt_program,
    });
    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitMarketParams {
    /// Encrypt program ID (4ebfzWdKnrnGseuQpezXdG8yCdHqwQ1SSBHD3bWArND8)
    pub encrypt_program:     Pubkey,
    /// Minimum collateral ratio in bps (e.g. 15000 = 150%)
    pub min_collateral_bps:  u16,
    /// Liquidation threshold in bps (e.g. 11000 = 110%)
    pub liquidation_bps:     u16,
}

#[derive(Accounts)]
pub struct InitMarket<'info> {
    #[account(
        init,
        payer = authority,
        space = Market::LEN,
        seeds = [
            b"market",
            collateral_mint.key().as_ref(),
            borrow_mint.key().as_ref(),
        ],
        bump,
    )]
    pub market: Account<'info, Market>,

    /// CHECK: any SPL mint for collateral
    pub collateral_mint: UncheckedAccount<'info>,
    /// CHECK: any SPL mint for the borrow token
    pub borrow_mint: UncheckedAccount<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[event]
pub struct MarketCreated {
    pub market:          Pubkey,
    pub collateral_mint: Pubkey,
    pub borrow_mint:     Pubkey,
    pub encrypt_program: Pubkey,
}
