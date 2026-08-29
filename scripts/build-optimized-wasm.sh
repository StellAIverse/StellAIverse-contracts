#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "Usage: $0 <package-name> [manifest-path]" >&2
    exit 1
fi

PACKAGE_NAME="$1"
MANIFEST_PATH="${2:-Cargo.toml}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [[ "$MANIFEST_PATH" != /* ]]; then
    MANIFEST_PATH="$REPO_ROOT/$MANIFEST_PATH"
fi

TARGET_DIR="$REPO_ROOT/target"
WASM_DIR="$TARGET_DIR/wasm32-unknown-unknown/release"
WASM_FILE="$WASM_DIR/${PACKAGE_NAME//-/_}.wasm"

echo "Building $PACKAGE_NAME with the release profile..."
CARGO_TARGET_DIR="$TARGET_DIR" cargo build \
    --manifest-path "$MANIFEST_PATH" \
    --release \
    --target wasm32-unknown-unknown \
    --package "$PACKAGE_NAME"

if [ ! -f "$WASM_FILE" ]; then
    echo "Expected wasm artifact not found: $WASM_FILE" >&2
    exit 1
fi

SIZE_BEFORE=$(wc -c < "$WASM_FILE")

if command -v wasm-opt >/dev/null 2>&1; then
    echo "Optimizing wasm with wasm-opt -Oz..."
    TEMP_FILE="$WASM_FILE.optimized"
    wasm-opt -Oz --strip-debug --strip-producers --enable-bulk-memory -o "$TEMP_FILE" "$WASM_FILE"
    mv "$TEMP_FILE" "$WASM_FILE"
elif command -v wasm-strip >/dev/null 2>&1; then
    echo "Stripping wasm with wasm-strip..."
    wasm-strip "$WASM_FILE"
else
    echo "wasm-opt and wasm-strip are unavailable; leaving the cargo-built wasm as-is."
fi

SIZE_AFTER=$(wc -c < "$WASM_FILE")
if [ "$SIZE_BEFORE" -gt 0 ]; then
    REDUCTION=$((100 - (SIZE_AFTER * 100 / SIZE_BEFORE)))
    echo "WASM size: $SIZE_BEFORE bytes -> $SIZE_AFTER bytes (${REDUCTION}% smaller)"
else
    echo "WASM size: $SIZE_AFTER bytes"
fi

echo "$WASM_FILE"