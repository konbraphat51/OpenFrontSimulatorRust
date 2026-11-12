// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/TerraNulliusImpl.ts: terra nullius is the small id 0, never an object.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::Game::SmallId;

/// Small id of terra nullius (`smallID()` upstream).
pub const SMALL_ID: SmallId = 0;

/// Placeholder client id of terra nullius (`clientID()` upstream).
pub const CLIENT_ID: &str = "TERRA_NULLIUS_CLIENT_ID";
