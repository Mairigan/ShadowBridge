//! request_decrypt.rs
//!
//! Two instructions:
//!   1. `request_decrypt` — CPI to Encrypt's request_decryption instruction.
//!      After the executor has committed an output ciphertext, this tells
//!      the Encrypt decryptor to reveal the plaintext. The decryptor writes
//!      a DecryptionResult account on-chain.
//!
//!   2. `finalize_open` — reads the DecryptionResult from the collateral check
//!      to learn whether collateral >= loan. If yes, marks the position Active.
//!
//!   3. `finalize_repayment` — reads the DecryptionResult from apply_repayment
//!      to learn whether the loan is fully repaid.
//!
//!   4. `close_position` — closes a Closed or Liquidated position, reclaims rent.

use anchor_lang::prelude::*;
use encrypt_anchor::EncryptCpi;
use crate::{
    errors::LendingError,
    state::{LendingPosition, Market, PositionStatus},
};

// ── 1. request_decrypt ────────────────────────────────────────────────────────

pub fn handler_request(ctx: Context<RequestDecrypt>) -> Result<()> {
    let position = &ctx.accounts.position;
    let clock    = Clock::get()?;

    require!(
        position.status == PositionStatus::PendingDecryption,
        LendingError::WrongState,
    );
    require!(
        position.collateral_check_output_ct != Pubkey::default(),
        LendingError::CollateralCheckMissing,
    );

    // Reject stale collateral checks (>= 150 slots ≈ ~60 seconds old)
    require!(
        clock.slot.saturating_sub(position.collateral_check_slot) < 150,
        LendingError::CollateralCheckStale,
    );

    // Verify the ciphertext account being decrypted matches what we stored
    require!(
        ctx.accounts.ciphertext_to_decrypt.key() == position.collateral_check_output_ct,
        LendingError::CiphertextMismatch,
    );

    // CPI: request_decryption on the Encrypt program.
    //
    // This tells the Encrypt decryptor to decrypt the given ciphertext
    // and write the plaintext to a DecryptionResult account.
    //
    // The `authorized` parameter restricts who can use the result —
    // we set it to this program's PDA so only our finalize_open
    // instruction can act on the decrypted value.
    ctx.accounts.encrypt_cpi.request_decryption(
        &ctx.accounts.encrypt_program.to_account_info(),
        &ctx.accounts.ciphertext_to_decrypt.to_account_info(),
        &ctx.accounts.decryption_request.to_account_info(),
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        ctx.accounts.program_authority.key(),
    )?;

    emit!(DecryptionRequested {
        position:   position.key(),
        ct_account: ctx.accounts.ciphertext_to_decrypt.key(),
    });
    Ok(())
}

#[derive(Accounts)]
pub struct RequestDecrypt<'info> {
    #[account(
        seeds = [b"market", market.collateral_mint.as_ref(), market.borrow_mint.as_ref()],
        bump  = market.bump,
    )]
    pub market: Account<'info, Market>,

    #[account(
        has_one = market,
        seeds = [
            b"position",
            market.key().as_ref(),
            position.borrower.as_ref(),
            &position.index.to_le_bytes(),
        ],
        bump = position.bump,
    )]
    pub position: Account<'info, LendingPosition>,

    /// CHECK: The ciphertext to decrypt — must match position.collateral_check_output_ct
    pub ciphertext_to_decrypt: UncheckedAccount<'info>,

    /// CHECK: New DecryptionRequest account — created by Encrypt
    #[account(mut)]
    pub decryption_request: UncheckedAccount<'info>,

    /// CHECK: This program's authority PDA — authorised to use the decryption result
    #[account(seeds = [b"prog_auth"], bump)]
    pub program_authority: UncheckedAccount<'info>,

    pub encrypt_cpi: EncryptCpi<'info>,

    /// CHECK: Encrypt program
    #[account(address = market.encrypt_program)]
    pub encrypt_program: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[event]
pub struct DecryptionRequested {
    pub position:   Pubkey,
    pub ct_account: Pubkey,
}

// ── 2. finalize_open ──────────────────────────────────────────────────────────
//
// Reads the DecryptionResult written by the Encrypt decryptor and either:
//   - Transitions position to Active (collateral sufficient), or
//   - Returns InsufficientCollateral error (borrower must close position)
//
// DecryptionResult account layout (from Encrypt docs):
//   [0..8]   discriminator
//   [8]      result type tag: 0 = bool result, 1 = u64 result
//   [9..17]  value as u64 LE (for bool: 0 = false, 1 = true)

pub fn handler_finalize_open(ctx: Context<FinalizeOpen>) -> Result<()> {
    let position = &mut ctx.accounts.position;

    require!(
        position.status == PositionStatus::PendingDecryption,
        LendingError::WrongState,
    );

    let data = ctx.accounts.decryption_result.data.borrow();
    require!(data.len() >= 17, LendingError::BadDecryptionResult);

    // type tag at byte 8: 0 = bool, 1 = u64
    // For EBool output: tag should be 0
    let value = u64::from_le_bytes(data[9..17].try_into().unwrap());
    let collateral_ok = value != 0;

    require!(collateral_ok, LendingError::InsufficientCollateral);

    position.status = PositionStatus::Active;

    emit!(PositionActivated {
        position: position.key(),
        borrower: position.borrower,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct FinalizeOpen<'info> {
    #[account(
        seeds = [b"market", market.collateral_mint.as_ref(), market.borrow_mint.as_ref()],
        bump  = market.bump,
    )]
    pub market: Account<'info, Market>,

    #[account(
        mut,
        has_one = market,
        has_one = borrower @ LendingError::WrongBorrower,
        seeds = [
            b"position",
            market.key().as_ref(),
            borrower.key().as_ref(),
            &position.index.to_le_bytes(),
        ],
        bump = position.bump,
    )]
    pub position: Account<'info, LendingPosition>,

    /// CHECK: DecryptionResult account written by the Encrypt decryptor.
    /// We read its raw bytes to extract the boolean result.
    pub decryption_result: UncheckedAccount<'info>,

    pub borrower: Signer<'info>,
}

#[event]
pub struct PositionActivated {
    pub position: Pubkey,
    pub borrower: Pubkey,
}

// ── 3. finalize_repayment ─────────────────────────────────────────────────────

pub fn handler_finalize_repayment(ctx: Context<FinalizeRepayment>) -> Result<()> {
    let position = &mut ctx.accounts.position;

    require!(
        position.status == PositionStatus::Repaying,
        LendingError::WrongState,
    );

    let data = ctx.accounts.decryption_result.data.borrow();
    require!(data.len() >= 17, LendingError::BadDecryptionResult);

    let value        = u64::from_le_bytes(data[9..17].try_into().unwrap());
    let fully_repaid = value != 0;

    if fully_repaid {
        position.status = PositionStatus::Closed;
        emit!(PositionClosed { position: position.key() });
    } else {
        // Partially repaid — back to Active with updated outstanding_ct
        position.status = PositionStatus::Active;
        emit!(PartialRepayment { position: position.key() });
    }
    Ok(())
}

#[derive(Accounts)]
pub struct FinalizeRepayment<'info> {
    #[account(
        seeds = [b"market", market.collateral_mint.as_ref(), market.borrow_mint.as_ref()],
        bump  = market.bump,
    )]
    pub market: Account<'info, Market>,

    #[account(
        mut,
        has_one = market,
        has_one = borrower @ LendingError::WrongBorrower,
        seeds = [
            b"position",
            market.key().as_ref(),
            borrower.key().as_ref(),
            &position.index.to_le_bytes(),
        ],
        bump = position.bump,
    )]
    pub position: Account<'info, LendingPosition>,

    /// CHECK: DecryptionResult for the apply_repayment graph's EBool output
    pub decryption_result: UncheckedAccount<'info>,

    pub borrower: Signer<'info>,
}

#[event]
pub struct PositionClosed { pub position: Pubkey }
#[event]
pub struct PartialRepayment { pub position: Pubkey }

// ── 4. close_position ─────────────────────────────────────────────────────────

pub fn handler_close(ctx: Context<ClosePosition>) -> Result<()> {
    let position = &ctx.accounts.position;

    require!(
        position.status == PositionStatus::Closed
            || position.status == PositionStatus::Liquidated,
        LendingError::WrongState,
    );
    Ok(()) // Anchor's `close = borrower` reclaims lamports automatically
}

#[derive(Accounts)]
pub struct ClosePosition<'info> {
    #[account(
        mut,
        has_one = borrower @ LendingError::WrongBorrower,
        close   = borrower, // returns rent to borrower
        seeds = [
            b"position",
            position.market.as_ref(),
            borrower.key().as_ref(),
            &position.index.to_le_bytes(),
        ],
        bump = position.bump,
    )]
    pub position: Account<'info, LendingPosition>,

    #[account(mut)]
    pub borrower: Signer<'info>,
}
