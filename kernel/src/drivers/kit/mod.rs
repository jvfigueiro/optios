// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: OptiOS Driver Kit Core Subsystem
// Date: 2026-11-10

//! OptiOS Driver Kit Core Subsystem.
//! Provides typed, safe abstractions (MMIO, DMA, Device Traits)
//! for native device drivers and foreign compatibility layers.

pub mod dma;
pub mod input;
pub mod mmio;
pub mod registry;

#[allow(unused_imports)]
pub use dma::DmaBuffer;
#[allow(unused_imports)]
pub use input::{InputEvent, KeyCode, MouseButtonKind};
pub use mmio::MmioRegion;
#[allow(unused_imports)]
pub use registry::{Diagnosable, DriverMetadata, DriverOrigin, register_driver};
