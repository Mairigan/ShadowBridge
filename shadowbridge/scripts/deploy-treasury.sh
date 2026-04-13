#!/usr/bin/env bash
# Deploy the treasury program (Ika dWallet, anchor 1.0)
set -euo pipefail

echo "═══ Deploying shadowbridge-treasury ═══"
echo "Anchor version: 1.0.0  (required by Ika SDK)"
echo ""

cd "$(dirname "$0")/../treasury"

# Ensure correct Anchor version
avm use 1.0.0

# Build
echo "[1/3] Building..."
anchor build

# Deploy
echo "[2/3] Deploying to devnet..."
anchor deploy --provider.cluster devnet

echo "[3/3] Update src/lib.rs declare_id! and Anchor.toml with the program ID above."
echo ""
echo "After updating, also set TREASURY_PROGRAM_ID in client/src/lib/treasury.ts"
echo ""
echo "Then run the one-time treasury setup:"
echo "  cd client && bun run src/bin/setup_treasury.ts"
