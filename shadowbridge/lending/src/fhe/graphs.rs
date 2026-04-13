//! fhe/graphs.rs
//!
//! FHE computation graphs for the lending market.
//!
//! Each `#[encrypt_fn]` function compiles into a computation graph (DAG of
//! FHE operations). The macro also generates a `<FUNCTION_NAME_UPPER>_GRAPH_ID`
//! constant — a [u8; 32] hash that identifies the graph for `execute_graph` CPI.
//!
//! TYPES (from encrypt-types)
//!   EUint64  — encrypted 64-bit unsigned integer
//!   EBool    — encrypted boolean
//!
//! SUPPORTED OPERATIONS in the DSL body:
//!   Arithmetic:  +, -, *
//!   Bit-shift:   << (left), >> (right)
//!   Comparison:  >=, >, <=, <, ==, !=
//!   Conditional: if/else (FHE mux — both branches always evaluated)
//!   Constants:   integer literals are treated as public graph parameters
//!
//! Off-chain execution flow:
//!   1. Client calls EncryptClient.createInput(value) via gRPC → gets ciphertext Pubkey
//!   2. Program stores ciphertext Pubkeys on-chain (in account fields)
//!   3. Program calls execute_graph CPI with graph_id + input ciphertext Pubkeys
//!   4. Encrypt executor picks up event, evaluates graph, commits output ciphertext
//!   5. Program calls request_decryption CPI when plaintext is needed
//!   6. Decryptor writes DecryptionResult account with the plaintext value

use encrypt_dsl::prelude::*;

// ── Graph 1: Collateral sufficiency check ─────────────────────────────────────
//
// Checks whether the collateral covers the requested loan.
// Both values are encrypted — the boolean result is also encrypted.
// Only the borrower (who holds the decryption key) can learn the result,
// unless the program explicitly requests decryption.
//
// The #[encrypt_fn] macro generates: CHECK_COLLATERAL_GRAPH_ID: [u8; 32]

#[encrypt_fn]
pub fn check_collateral(collateral: EUint64, loan: EUint64) -> EBool {
    collateral >= loan
}

// ── Graph 2: Repayment application ────────────────────────────────────────────
//
// Computes new outstanding balance after a repayment, and whether the loan
// is fully repaid. Both the outstanding balance and the repayment amount
// are encrypted inputs; the results are encrypted outputs.
//
// Outputs: (remaining_balance: EUint64, fully_repaid: EBool)
//
// Note: FHE `if/else` is a multiplexer — both branches are evaluated.
// This is correct and expected behaviour in FHE.

#[encrypt_fn]
pub fn apply_repayment(
    outstanding: EUint64,
    repayment: EUint64,
) -> (EUint64, EBool) {
    let fully_repaid = repayment >= outstanding;
    // FHE mux: if fully_repaid then 0 else (outstanding - repayment)
    // We avoid `outstanding - repayment` underflow when fully_repaid is true
    // by selecting 0 in that branch. The subtraction in the else branch is
    // safe because it only "takes effect" when repayment < outstanding.
    let remaining = if fully_repaid {
        outstanding - outstanding // == 0, without needing a literal zero
    } else {
        outstanding - repayment
    };
    (remaining, fully_repaid)
}

// ── Graph 3: Liquidation eligibility check ────────────────────────────────────
//
// Determines whether a position is eligible for liquidation.
// Liquidation is allowed when: collateral_value < loan * min_ratio / 10_000
//
// min_ratio_bps is passed as a public constant in the graph call params.
// We approximate division by 10_000 using a right-shift:
//   x / 10_000 ≈ x >> 13  (2^13 = 8192, error ≈ 18%)  — too coarse
//   Better: x * 11_000 / 10_000 = x + x/10 ≈ x + (x >> 3)  (for 110%)
//
// For production, use a fixed-point arithmetic library. For the pre-alpha
// demo we use the exact multiply approach which is accurate for any ratio:
//   threshold = loan_outstanding * min_ratio_bps
//   liquidatable = (collateral_value * 10_000) < threshold

#[encrypt_fn]
pub fn is_liquidatable(
    collateral_value: EUint64,
    outstanding: EUint64,
    min_ratio_bps: EUint64, // public constant passed at graph execution time
) -> EBool {
    // collateral_value * 10_000 < outstanding * min_ratio_bps
    // Both sides encrypted — no plaintext leaks
    let lhs = collateral_value * 10_000_u64;
    let rhs = outstanding * min_ratio_bps;
    lhs < rhs
}
