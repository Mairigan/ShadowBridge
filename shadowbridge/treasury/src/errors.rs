use anchor_lang::prelude::*;

// Anchor v1 enforces exactly one #[error_code] block per program.
#[error_code]
pub enum TreasuryError {
    #[msg("Treasury is not active")]
    NotActive,
    #[msg("Caller is not the treasury admin")]
    NotAdmin,
    #[msg("Caller is not a registered member")]
    NotMember,
    #[msg("Proposal is not open for voting")]
    NotOpen,
    #[msg("Proposal has expired")]
    Expired,
    #[msg("Proposal has already been executed")]
    AlreadyExecuted,
    #[msg("Amount exceeds the per-proposal spending cap")]
    ExceedsCap,
    #[msg("Quorum value is invalid (must be 1 ≤ quorum ≤ member_count)")]
    BadQuorum,
    #[msg("The dWallet does not match the treasury record")]
    DWalletMismatch,
    #[msg("MessageApproval account data is too short")]
    BadApprovalData,
    #[msg("MessageApproval message_hash does not match proposal")]
    ApprovalHashMismatch,
    #[msg("Arithmetic overflow")]
    Overflow,
}
