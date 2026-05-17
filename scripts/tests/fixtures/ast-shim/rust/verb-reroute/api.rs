// Negative fixture for the V05_VERBS re-route rule. `configure_fts`
// was an AdminClient verb in 0.5.x; in 0.6.0 it MUST NOT exist as a
// public symbol — collapsed into `admin::configure(name, body)`.
pub fn configure_fts() {}
