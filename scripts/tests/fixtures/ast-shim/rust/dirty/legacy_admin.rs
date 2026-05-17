// Negative fixture for AC-050a: forbidden module name + crate-root
// suppression. Lives only under scripts/tests/fixtures/ so the real
// scan never trips on it.
#![allow(deprecated)]

pub mod compat_v0_5_admin {}

pub fn legacy_open() {}
