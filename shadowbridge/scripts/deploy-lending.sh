#!/usr/bin/env bash
# Deploy the lending program (Encrypt, anchor 0.32)
set -euo pipefail

echo "═══ Deploying shadowbridge-lending ═══"
echo "Anchor version: 0.32.0  (required by Encrypt SDK)"
echo ""

cd "$(dirname "$0")/../lending"

# Ensure correct Anchor version
avm use 0.32.0

# Build
echo "[1/3] Building..."
anchor build

# Deploy
echo "[2/3] Deploying to devnet..."
anchor deploy --provider.cluster devnet

# Extract program ID from deploy output and update declare_id!
echo "[3/3] Update src/lib.rs declare_id! and Anchor.toml with the program ID above."
echo ""
echo "After updating, also set LENDING_PROGRAM_ID in client/src/lib/lending.ts"
