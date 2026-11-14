// SPDX-License-Identifier: AGPL-3.0-only
//! OpenFront game records: the JSON schema, gzip loading and expansion of sparse turns.

#![allow(non_snake_case)]

pub mod Canonical;
pub mod Config;
pub mod Decompress;
pub mod Intents;
pub mod Loader;
pub mod Records;
pub mod StartInfo;
pub mod WireEnum;
