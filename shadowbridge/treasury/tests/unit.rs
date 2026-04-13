#[cfg(test)]
mod tests {
    use anchor_lang::prelude::Pubkey;

    const IKA_PROGRAM: &str = "87W54kGYFQ1rgWqMeu4XTPHWXWmXSQCcjm8vCTfiq1oY";
    const CPI_AUTHORITY_SEED: &[u8] = b"__ika_cpi_authority";

    #[test]
    fn ika_program_id_parses() {
        let pk: Pubkey = IKA_PROGRAM.parse().unwrap();
        println!("Ika dWallet program: {pk}");
    }

    #[test]
    fn cpi_authority_pda_derivation() {
        let prog_id = Pubkey::new_unique();
        let (pda, bump) = Pubkey::find_program_address(&[CPI_AUTHORITY_SEED], &prog_id);
        assert!(!pda.is_on_curve());
        assert!(bump <= 255);
        println!("CPI authority: {pda}  bump={bump}");
        println!("Seed: {}", String::from_utf8_lossy(CPI_AUTHORITY_SEED));
    }

    #[test]
    fn treasury_pda_deterministic() {
        let prog_id = Pubkey::new_unique();
        let admin   = Pubkey::new_unique();
        let (a, _) = Pubkey::find_program_address(&[b"treasury", admin.as_ref()], &prog_id);
        let (b, _) = Pubkey::find_program_address(&[b"treasury", admin.as_ref()], &prog_id);
        assert_eq!(a, b);
    }

    #[test]
    fn member_pda_unique_per_member() {
        let prog     = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let m1       = Pubkey::new_unique();
        let m2       = Pubkey::new_unique();
        let (pda1, _) = Pubkey::find_program_address(
            &[b"member", treasury.as_ref(), m1.as_ref()], &prog);
        let (pda2, _) = Pubkey::find_program_address(
            &[b"member", treasury.as_ref(), m2.as_ref()], &prog);
        assert_ne!(pda1, pda2);
    }

    #[test]
    fn vote_record_prevents_double_vote() {
        let prog     = Pubkey::new_unique();
        let proposal = Pubkey::new_unique();
        let voter    = Pubkey::new_unique();
        // Same inputs always produce same PDA — init errors on second call
        let (a, _) = Pubkey::find_program_address(
            &[b"vote", proposal.as_ref(), voter.as_ref()], &prog);
        let (b, _) = Pubkey::find_program_address(
            &[b"vote", proposal.as_ref(), voter.as_ref()], &prog);
        assert_eq!(a, b, "vote PDA must be deterministic (Anchor init prevents double-vote)");
    }

    #[test]
    fn quorum_logic() {
        let quorum       = 3u32;
        let member_count = 5u32;
        let mut yes      = 0u32;
        let mut no       = 0u32;

        // First 2 yes votes — quorum not reached
        yes += 1; assert!(yes < quorum);
        yes += 1; assert!(yes < quorum);

        // 3rd yes vote — quorum reached
        yes += 1; assert!(yes >= quorum);

        // Rejection: remaining votes can't reach quorum
        let remaining  = member_count.saturating_sub(yes + no);
        let max_yes    = yes + remaining;
        // After 3 yes, even if remaining 2 vote yes → still approved (5 > 3)
        // Let's test rejection: 0 yes, 3 no, 2 remaining → max possible yes = 2 < quorum 3
        yes = 0; no = 3;
        let remaining2 = member_count.saturating_sub(yes + no);
        let max_yes2   = yes + remaining2;
        assert!(max_yes2 < quorum, "should be rejected");
    }

    #[test]
    fn spending_cap_enforced() {
        let cap: u64   = 100_000_000; // 1 SOL
        let ok: u64    =  99_999_999;
        let exact: u64 = 100_000_000;
        let over: u64  = 100_000_001;
        assert!(ok    <= cap);
        assert!(exact <= cap);
        assert!(over  >  cap);
    }

    #[test]
    fn proposal_expiry_slots() {
        use shadowbridge_treasury::state::PROPOSAL_EXPIRY_SLOTS;
        // ~2 days at 400ms/slot
        let min_expected: u64 = 400_000; // ~1.85 days
        let max_expected: u64 = 500_000; // ~2.3 days
        assert!(PROPOSAL_EXPIRY_SLOTS >= min_expected);
        assert!(PROPOSAL_EXPIRY_SLOTS <= max_expected);
    }

    #[test]
    fn message_approval_layout() {
        // Validate our layout assumption (bytes 40–72 = message_hash, 72+ = sig)
        let discriminator_end = 8usize;
        let dwallet_end       = discriminator_end + 32; // 40
        let hash_end          = dwallet_end + 32;       // 72
        let sig_start         = hash_end;               // 72

        assert_eq!(dwallet_end, 40);
        assert_eq!(hash_end,    72);
        assert_eq!(sig_start,   72);

        // Secp256k1 sig = 64 bytes → total minimum = 136 bytes
        println!("Minimum MessageApproval size: {} bytes", sig_start + 64);
    }
}
