use anchor_lang::prelude::*;

#[error_code]
pub enum LendingError {
    #[msg("Market is paused")]
    MarketPaused,

    #[msg("Caller is not the market authority")]
    Unauthorized,

    #[msg("Position is not in the expected state")]
    WrongState,

    #[msg("Position belongs to a different borrower")]
    WrongBorrower,

    #[msg("The ciphertext account is not owned by the Encrypt program")]
    BadCiphertextOwner,

    #[msg("The ciphertext account key does not match the stored reference")]
    CiphertextMismatch,

    #[msg("Collateral check graph has not been run yet")]
    CollateralCheckMissing,

    #[msg("Collateral check is too old — re-run before finalising")]
    CollateralCheckStale,

    #[msg("Decryption result account has unexpected data length")]
    BadDecryptionResult,

    #[msg("Decrypted collateral check: insufficient collateral")]
    InsufficientCollateral,

    #[msg("Position is not liquidatable (collateral above threshold)")]
    NotLiquidatable,

    #[msg("Arithmetic overflow")]
    Overflow,
}
