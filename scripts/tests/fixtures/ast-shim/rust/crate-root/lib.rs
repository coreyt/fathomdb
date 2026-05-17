// Isolates the crate-root #![allow(deprecated)] rule. The filename is
// lib.rs precisely because the rule only fires for lib.rs / main.rs;
// putting this attribute in any other file would not trigger AC-050a.
#![allow(deprecated)]
