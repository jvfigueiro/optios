// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: OptiOS Driver Kit Core Subsystem
// Date: 2026-11-10

//! OptiOS Driver Kit Core Subsystem.
//! Provides typed, safe abstractions (MMIO, DMA, Device Traits)
//! for native device drivers and foreign compatibility layers.

pub mod dma;
pub mod mmio;

#[allow(unused_imports)]
pub use dma::DmaBuffer;
pub use mmio::MmioRegion;
