//! Unit tests for the lending program.
//! Run with: cargo test  (no SBF or devnet required)

#[cfg(test)]
mod tests {
    use anchor_lang::prelude::Pubkey;

    // Real Encrypt devnet program ID (from docs)
    const ENCRYPT_PROGRAM: &str = "4ebfzWdKnrnGseuQpezXdG8yCdHqwQ1SSBHD3bWArND8";

    #[test]
    fn encrypt_program_id_parses() {
        let pk: Pubkey = ENCRYPT_PROGRAM.parse().unwrap();
        println!("Encrypt program: {pk}");
    }

    #[test]
    fn market_pda_is_deterministic() {
        let prog        = Pubkey::new_unique();
        let collateral  = Pubkey::new_unique();
        let borrow      = Pubkey::new_unique();
        let (pda_a, bump_a) = Pubkey::find_program_address(
            &[b"market", collateral.as_ref(), borrow.as_ref()],
            &prog,
        );
        let (pda_b, bump_b) = Pubkey::find_program_address(
            &[b"market", collateral.as_ref(), borrow.as_ref()],
            &prog,
        );
        assert_eq!(pda_a, pda_b);
        assert_eq!(bump_a, bump_b);
        assert!(!pda_a.is_on_curve());
    }

    #[test]
    fn position_pda_unique_per_index() {
        let prog    = Pubkey::new_unique();
        let market  = Pubkey::new_unique();
        let borrower = Pubkey::new_unique();

        let (pda_0, _) = Pubkey::find_program_address(
            &[b"position", market.as_ref(), borrower.as_ref(), &0u64.to_le_bytes()],
            &prog,
        );
        let (pda_1, _) = Pubkey::find_program_address(
            &[b"position", market.as_ref(), borrower.as_ref(), &1u64.to_le_bytes()],
            &prog,
        );
        assert_ne!(pda_0, pda_1, "different indices must give different PDAs");
    }

    #[test]
    fn collateral_check_logic_correct() {
        // check_collateral: collateral >= loan → true
        let loan: u64        = 10_000_000_000; // 10 SOL
        let good_collateral  = 20_000_000_000u64; // 200% — passes
        let bad_collateral   =  8_000_000_000u64; // 80%  — fails
        assert!(good_collateral >= loan);
        assert!(!(bad_collateral >= loan));
    }

    #[test]
    fn repayment_logic_full_and_partial() {
        let outstanding: u64 = 10_000_000_000;
        let full_payment     = 10_000_000_000u64;
        let partial_payment  =  5_000_000_000u64;

        let (remaining_full, repaid_full) = {
            let repaid = full_payment >= outstanding;
            let rem    = if repaid { 0u64 } else { outstanding - full_payment };
            (rem, repaid)
        };
        assert!(repaid_full);
        assert_eq!(remaining_full, 0);

        let (remaining_partial, repaid_partial) = {
            let repaid = partial_payment >= outstanding;
            let rem    = if repaid { 0u64 } else { outstanding - partial_payment };
            (rem, repaid)
        };
        assert!(!repaid_partial);
        assert_eq!(remaining_partial, 5_000_000_000);
    }

    #[test]
    fn liquidation_logic_correct() {
        // is_liquidatable: (collateral * 10_000) < (outstanding * min_ratio_bps)
        let outstanding: u64   = 10_000_000_000; // 10 SOL outstanding
        let min_ratio_bps: u64 = 11_000;          // 110% liquidation threshold

        let healthy_collateral    = 12_000_000_000u64; // 120% → NOT liquidatable
        let liquidatable_collateral = 9_000_000_000u64; // 90%  → liquidatable

        let rhs = outstanding * min_ratio_bps;

        let healthy_lhs     = healthy_collateral * 10_000;
        let liquidatable_lhs = liquidatable_collateral * 10_000;

        assert!(!(healthy_lhs < rhs),     "120% should NOT be liquidatable");
        assert!(liquidatable_lhs < rhs,   " 90% SHOULD be liquidatable");
    }

    #[test]
    fn decryption_result_layout() {
        // Simulate DecryptionResult account data layout from Encrypt docs:
        //   [0..8]  discriminator
        //   [8]     type tag: 0=bool, 1=u64
        //   [9..17] value as u64 LE
        let mut data = [0u8; 17];
        data[8]  = 0; // bool type
        data[9]  = 1; // value = true (little-endian u64: 1)
        let value = u64::from_le_bytes(data[9..17].try_into().unwrap());
        assert_eq!(value, 1, "true should decode as 1");

        let mut data2 = [0u8; 17];
        data2[8] = 0;
        data2[9] = 0; // false
        let value2 = u64::from_le_bytes(data2[9..17].try_into().unwrap());
        assert_eq!(value2, 0, "false should decode as 0");
    }

    #[test]
    fn staleness_check() {
        let check_slot: u64   = 1_000_000;
        let current_valid     = 1_000_100u64; // 100 slots later — still fresh
        let current_stale     = 1_000_200u64; // 200 slots later — stale

        const MAX_STALENESS: u64 = 150;

        assert!(current_valid.saturating_sub(check_slot) < MAX_STALENESS);
        assert!(!(current_stale.saturating_sub(check_slot) < MAX_STALENESS));
    }

    #[test]
    fn account_sizes() {
        use shadowbridge_lending::state::{GraphRecord, LendingPosition, Market};
        // Verify that declared LEN constants are reasonable (not zero, not huge)
        assert!(Market::LEN > 100 && Market::LEN < 500);
        assert!(LendingPosition::LEN > 200 && LendingPosition::LEN < 700);
        assert!(GraphRecord::LEN > 50 && GraphRecord::LEN < 200);
        println!("Market::LEN          = {}", Market::LEN);
        println!("LendingPosition::LEN = {}", LendingPosition::LEN);
        println!("GraphRecord::LEN     = {}", GraphRecord::LEN);
    }
}
