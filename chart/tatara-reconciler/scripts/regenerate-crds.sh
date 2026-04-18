#!/usr/bin/env bash
# Regenerate the CRDs under ./crds/ from the current tatara-process Rust types.
# Run from anywhere — resolves workspace root relative to this script.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHART_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_ROOT="$(cd "$CHART_DIR/../.." && pwd)"
CRDS_DIR="$CHART_DIR/crds"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

cd "$WORKSPACE_ROOT"

echo "building tatara-crd-gen..."
cargo build --quiet -p tatara-reconciler --bin tatara-crd-gen

echo "generating CRDs..."
./target/debug/tatara-crd-gen > "$TMP/all.yaml"

awk '
  BEGIN { doc = 0 }
  /^---$/ { doc++; next }
  doc == 1 { print > "'"$TMP"'/processes.yaml" }
  doc == 2 { print > "'"$TMP"'/processtables.yaml" }
' "$TMP/all.yaml"

mkdir -p "$CRDS_DIR"
mv "$TMP/processes.yaml"      "$CRDS_DIR/processes.yaml"
mv "$TMP/processtables.yaml"  "$CRDS_DIR/processtables.yaml"

echo "wrote:"
ls -la "$CRDS_DIR"
