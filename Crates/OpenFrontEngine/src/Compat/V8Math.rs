//! Placeholder for V8's fdlibm `exp`, `log`, `pow`, `atan2`, `sin`, `cos` (src/base/ieee754.cc).
//!
//! Builds before `DetMath` call the engine's native `Math.*`; a later phase ports those routines
//! here bit for bit. Rust's `f64` transcendental methods must never stand in for them.
