//! open_position.rs
//!
//! Register a new encrypted lending position.
//!
//! Before calling this, the borrower must:
//!   1. Call `EncryptClient.createInput(loanAmount, programId)` via gRPC
//!      → receives `loan_ct` (Pubkey of an Encrypt-owned ciphertext account)
//!   2. Call `EncryptClient.createInput(collateralAmount, programId)` via gRPC
//!      → receives `collateral_ct`
//!   3. Call this instruction with both Pubkeys
//!
//! This instruction only registers the ciphertext references. It does NOT
//! disburse funds. The borrower must then call execute_graph (CollateralCheck),
//! wait for the executor, call request_decrypt, wait for the decryptor, then
//! call finalize_open.

use anchor_lang::prelude::*;
use crate::{
    errors::LendingError,
    state::{LendingPosition, Market, PositionStatus},
};

pub fn handler(ctx: Context<OpenPosition>) -> Result<()> {
    let market   = &mut ctx.accounts.market;
    let position = &mut ctx.accounts.position;
    let clock    = Clock::get()?;

    require!(market.active, LendingError::MarketPaused);

    // Verify ciphertext accounts are owned by the Encrypt program.
    // Encrypt creates and owns all ciphertext accounts it manages.
    require!(
        ctx.accounts.loan_ct.owner == &market.encrypt_program,
        LendingError::BadCiphertextOwner,
    );
    require!(
        ctx.accounts.collateral_ct.owner == &market.encrypt_program,
        LendingError::BadCiphertextOwner,
    );

    position.borrower                  = ctx.accounts.borrower.key();
    position.market                    = market.key();
    position.loan_ct                   = ctx.accounts.loan_ct.key();
    position.collateral_ct             = ctx.accounts.collateral_ct.key();
    position.outstanding_ct            = Pubkey::default();
    position.collateral_check_output_ct = Pubkey::default();
    position.collateral_check_slot     = 0;
    position.liquidation_check_output_ct = Pubkey::default();
    position.opened_at                 = clock.unix_timestamp;
    position.status                    = PositionStatus::PendingCheck;
    position.index                     = market.position_count;
    position.bump                      = ctx.bumps.position;

    market.position_count = market
        .position_count
        .checked_add(1)
        .ok_or(LendingError::Overflow)?;

    emit!(PositionOpened {
        position:      position.key(),
        borrower:      position.borrower,
        market:        position.market,
        loan_ct:       position.loan_ct,
        collateral_ct: position.collateral_ct,
        index:         position.index,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct OpenPosition<'info> {
    #[account(
        mut,
        seeds = [
            b"market",
            market.collateral_mint.as_ref(),
            market.borrow_mint.as_ref(),
        ],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,

    #[account(
        init,
        payer = borrower,
        space = LendingPosition::LEN,
        seeds = [
            b"position",
            market.key().as_ref(),
            borrower.key().as_ref(),
            &market.position_count.to_le_bytes(),
        ],
        bump,
    )]
    pub position: Account<'info, LendingPosition>,

    /// CHECK: Encrypt-owned ciphertext account for the loan amount.
    /// Owner verified in handler: must equal market.encrypt_program.
    pub loan_ct: UncheckedAccount<'info>,

    /// CHECK: Encrypt-owned ciphertext account for the collateral amount.
    pub collateral_ct: UncheckedAccount<'info>,

    #[account(mut)]
    pub borrower: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[event]
pub struct PositionOpened {
    pub position:      Pubkey,
    pub borrower:      Pubkey,
    pub market:        Pubkey,
    pub loan_ct:       Pubkey,
    pub collateral_ct: Pubkey,
    pub index:         u64,
}
