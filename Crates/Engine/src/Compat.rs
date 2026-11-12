// SPDX-License-Identifier: AGPL-3.0-only
//! JS semantics the engine must reproduce: number conversions, ordered collections, V8 math.

pub mod HostMath;
pub mod JsNumber;
pub mod OrderedMap;
pub mod OrderedSet;
#[allow(
    clippy::excessive_precision,
    clippy::manual_range_contains,
    clippy::eq_op,
    clippy::approx_constant,
    clippy::explicit_counter_loop,
    clippy::if_same_then_else,
    clippy::type_complexity
)]
pub mod V8Math;
