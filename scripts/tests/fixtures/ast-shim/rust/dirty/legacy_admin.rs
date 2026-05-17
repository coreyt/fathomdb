// Negative fixture for AC-050a: forbidden module name + public-symbol
// prefix. The crate-root #![allow(deprecated)] rule is exercised by
// the dedicated lib.rs fixture in this directory — keep that rule
// separate so a regression in either path can't be masked by the other.

pub mod compat_v0_5_admin {}

pub fn legacy_open() {}
