#!/usr/bin/env bash
# OptiOS
# Copyright (C) 2026 João Figueiró. All rights reserved
# Item: Automated Host Test Runner for Library Crates
# Date: 2026-11-10

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${PROJECT_ROOT}"

echo "=================================================================="
echo " OptiOS Host Test Suite"
echo " Crates: optios-ustar, optios-ahci-proto, optios-partition-table, optios-elf-loader, optios-ansi-parser, optios-wm"
echo " Target: x86_64-unknown-linux-gnu"
echo "=================================================================="

# Note: optios-kernel is a #![no_std] bare-metal kernel with a custom #[panic_handler].
# Whole-workspace `cargo test` fails due to panic_impl symbol collisions with the std
# test harness. Host unit tests are executed exclusively for the pure library crates.
cargo test -p optios-ustar -p optios-ahci-proto -p optios-partition-table -p optios-elf-loader -p optios-ansi-parser -p optios-wm --target x86_64-unknown-linux-gnu "$@"

echo "=================================================================="
echo " All host unit tests passed successfully."
echo "=================================================================="
