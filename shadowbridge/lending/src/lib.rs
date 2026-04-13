//! shadowbridge_lending — Encrypt FHE encrypted lending market
//!
//! Uses anchor-lang 0.32 (required by encrypt-anchor).
//! Encrypt program on devnet: 4ebfzWdKnrnGseuQpezXdG8yCdHqwQ1SSBHD3bWArND8
//! Encrypt gRPC: https://pre-alpha-dev-1.encrypt.ika-network.net:443

use anchor_lang::prelude::*;

pub mod errors;
pub mod fhe;
pub mod instructions;
pub mod state;

use instructions::*;
use state::GraphKind;

// Replace with real ID after: anchor deploy --provider.cluster devnet
declare_id!("LendXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");

#[program]
pub mod shadowbridge_lending {
    use super::*;

    /// Initialise a new lending market for a collateral/borrow token pair.
    pub fn init_market(
        ctx: Context<InitMarket>,
        params: init_market::InitMarketParams,
    ) -> Result<()> {
        init_market::handler(ctx, params)
    }

    /// Register a new encrypted lending position.
    ///
    /// The borrower must first submit loan_amount and collateral_amount to the
    /// Encrypt gRPC service to obtain ciphertext account Pubkeys, then pass
    /// those Pubkeys here via the loan_ct and collateral_ct accounts.
    pub fn open_position(ctx: Context<OpenPosition>) -> Result<()> {
        open_position::handler(ctx)
    }

    /// Execute an FHE computation graph via CPI to the Encrypt program.
    ///
    /// `kind` selects which #[encrypt_fn] graph to run:
    ///   - CollateralCheck: check_collateral(collateral, loan) → EBool
    ///   - Repayment:       apply_repayment(outstanding, payment) → (EUint64, EBool)
    ///   - LiquidationCheck: is_liquidatable(collateral_val, outstanding, ratio) → EBool
    ///
    /// The Encrypt executor commits the output ciphertext asynchronously.
    /// Poll the output_ct account for data, then call request_decrypt.
    pub fn execute_graph(ctx: Context<ExecuteGraph>, kind: GraphKind) -> Result<()> {
        execute_graph::handler(ctx, kind)
    }

    /// Request decryption of the collateral-check output ciphertext.
    ///
    /// Call after the Encrypt executor has committed the check_collateral output.
    /// The Encrypt decryptor will write a DecryptionResult account on-chain.
    pub fn request_decrypt(ctx: Context<RequestDecrypt>) -> Result<()> {
        request_decrypt::handler_request(ctx)
    }

    /// Finalise opening a position after the decryption result is available.
    ///
    /// Reads the DecryptionResult boolean. If collateral >= loan, transitions
    /// the position to Active. Otherwise returns InsufficientCollateral.
    pub fn finalize_open(ctx: Context<FinalizeOpen>) -> Result<()> {
        request_decrypt::handler_finalize_open(ctx)
    }

    /// Finalise a repayment after the apply_repayment graph decryption result.
    pub fn finalize_repayment(ctx: Context<FinalizeRepayment>) -> Result<()> {
        request_decrypt::handler_finalize_repayment(ctx)
    }

    /// Close a fully repaid or liquidated position and reclaim rent.
    pub fn close_position(ctx: Context<ClosePosition>) -> Result<()> {
        request_decrypt::handler_close(ctx)
    }
}
