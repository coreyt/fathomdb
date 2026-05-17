// Nested-block-comment safety fixture for AC-050a.
/* outer /* legacy_inner */ still outer — must not flag */
pub fn safe_function() {}
