//! Compatibility shims that reproduce JavaScript runtime behaviour bit for bit.

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
