#!/bin/bash

# BitOSDT 2.0 Release Build Script

set -e

VERSION="2.0.0"
BUILD_DIR="target/release"
DIST_DIR="dist"

echo "=========================================="
echo "BitOSDT ${VERSION} Release Build"
echo "=========================================="
echo ""

# Clean previous builds
echo "[1/6] Cleaning previous builds..."
cargo clean

# Build CLI version
echo "[2/6] Building CLI version..."
cargo build --release

# Run tests
echo "[3/6] Running tests..."
cargo test --release

# Check formatting
echo "[4/6] Checking code formatting..."
cargo fmt -- --check

# Run clippy
echo "[5/6] Running clippy..."
cargo clippy --release -- -D warnings

# Create distribution
echo "[6/6] Creating distribution..."
mkdir -p ${DIST_DIR}

# Copy binary
cp ${BUILD_DIR}/bitosdt ${DIST_DIR}/

# Copy documentation
cp README.md ${DIST_DIR}/
cp CHANGELOG.md ${DIST_DIR}/

# Copy license (create if doesn't exist)
if [ -f LICENSE ]; then
    cp LICENSE ${DIST_DIR}/
else
    echo "MIT License" > ${DIST_DIR}/LICENSE
fi

# Create archive
cd ${DIST_DIR}
tar -czf "../bitosdt-${VERSION}-linux-x64.tar.gz" *
cd ..

echo ""
echo "=========================================="
echo "Build Complete!"
echo "=========================================="
echo "Binary: ${DIST_DIR}/bitosdt"
echo "Archive: bitosdt-${VERSION}-linux-x64.tar.gz"
echo ""
echo "To install locally:"
echo "  sudo cp ${DIST_DIR}/bitosdt /usr/local/bin/"
echo "  bitosdt init"
echo ""
echo "To test:"
echo "  bitosdt --version"
echo "  bitosdt info"
echo "=========================================="