pub const SCHEMA_VERSION: u32 = 1;

#[must_use]
pub fn bootstrap_steps() -> &'static [&'static str] {
    &["create canonical tables", "register projection metadata", "seed rewrite-era configuration"]
}
