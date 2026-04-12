//! Phase 3 adaptive-search relaxed-branch derivation.
//!
//! The coordinator first runs the strict [`TextQuery`] the caller typed. When
//! the strict branch returns fewer than `min(FALLBACK_TRIGGER_K, limit)` hits
//! (with `K = 1` in v1, i.e. strict was empty), a relaxed branch is derived
//! from the same query and executed as a second pass. This module owns the
//! pure Rust policy: turning a strict [`TextQuery`] into an optional relaxed
//! [`TextQuery`] plus a "was degraded at plan time" flag.
//!
//! Derivation rules (Phase 3, deliberately conservative):
//!
//! - An implicit-AND root (`And([a, b, c])`) is rewritten as per-term
//!   alternatives (`Or([a, b, c])`).
//! - Quoted phrases survive as a single alternative: `Phrase(..)` is not
//!   broken back into words.
//! - Top-level `Not(..)` atoms are **dropped**: exclusion is softened to
//!   "don't exclude." If dropping all `Not` children leaves nothing (or a
//!   single non-Not child that no longer benefits from relaxation), the
//!   derivation returns `None`.
//! - An already-`Or(..)` query at the root has no useful relaxation and
//!   returns `None`.
//! - A single-term query (`Term(..)` / `Phrase(..)` / `And([x])`) has no
//!   useful relaxation and returns `None`.
//! - Anything nested more deeply than the top level is NOT recursively
//!   relaxed in Phase 3 — return `None`. Future phases may extend this.
//!
//! The number of alternatives in the derived OR is capped at
//! [`RELAXED_BRANCH_CAP`]. If the cap truncates the plan, the returned
//! `was_degraded` flag is `true`. Truncation is order-preserving: the first
//! [`RELAXED_BRANCH_CAP`] alternatives (in token order) are kept.

use crate::TextQuery;

/// Maximum number of alternatives the relaxed OR-branch may hold before it is
/// truncated. Phase 3 caps at 4; truncation marks the plan "degraded."
pub const RELAXED_BRANCH_CAP: usize = 4;

/// Fallback trigger threshold. The relaxed branch runs when the strict branch
/// returned fewer than `min(FALLBACK_TRIGGER_K, limit)` hits. Phase 3 pins
/// `K = 1`, collapsing the rule to "run relaxed iff strict was empty," but
/// the coordinator still computes the `min(K, limit)` form explicitly so a
/// future policy change is a single constant bump.
pub const FALLBACK_TRIGGER_K: usize = 1;

/// Derive the relaxed-branch query for a strict [`TextQuery`].
///
/// Returns `(Some(relaxed), was_degraded)` when a useful relaxation exists,
/// or `(None, false)` when the strict query has no useful relaxation (already
/// an OR, single-term, nested, or empty after dropping top-level `Not`s).
///
/// `was_degraded` is `true` iff the alternatives count exceeded
/// [`RELAXED_BRANCH_CAP`] and the relaxed branch was order-preservingly
/// truncated to the first [`RELAXED_BRANCH_CAP`] alternatives.
#[must_use]
pub fn derive_relaxed(strict: &TextQuery) -> (Option<TextQuery>, bool) {
    // Anything that isn't an implicit-AND at the root has no useful Phase 3
    // relaxation. This covers Empty, Term, Phrase, Or, and top-level Not.
    let children = match strict {
        TextQuery::And(children) => children,
        TextQuery::Empty
        | TextQuery::Term(_)
        | TextQuery::Phrase(_)
        | TextQuery::Or(_)
        | TextQuery::Not(_) => return (None, false),
    };

    // Drop top-level Not(..) children: exclusion is softened.
    let mut kept: Vec<TextQuery> = Vec::with_capacity(children.len());
    for child in children {
        match child {
            // Only lift "simple" atoms into the alternatives list. If a
            // top-level AND holds a nested And/Or, Phase 3 does not descend —
            // treat the entire plan as un-relaxable. Top-level Not and Empty
            // children are silently dropped.
            TextQuery::Term(_) | TextQuery::Phrase(_) => kept.push(child.clone()),
            TextQuery::Not(_) | TextQuery::Empty => {}
            TextQuery::And(_) | TextQuery::Or(_) => return (None, false),
        }
    }

    // No useful relaxation for zero- or single-alternative results.
    if kept.len() < 2 {
        return (None, false);
    }

    // Order-preserving cap at RELAXED_BRANCH_CAP.
    let was_degraded = kept.len() > RELAXED_BRANCH_CAP;
    if was_degraded {
        kept.truncate(RELAXED_BRANCH_CAP);
    }

    (Some(TextQuery::Or(kept)), was_degraded)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn term(s: &str) -> TextQuery {
        TextQuery::Term(s.to_owned())
    }

    fn phrase(s: &str) -> TextQuery {
        TextQuery::Phrase(s.to_owned())
    }

    #[test]
    fn derive_relaxed_breaks_implicit_and_into_or() {
        let strict = TextQuery::And(vec![term("a"), term("b"), term("c")]);
        let (relaxed, was_degraded) = derive_relaxed(&strict);
        assert_eq!(
            relaxed,
            Some(TextQuery::Or(vec![term("a"), term("b"), term("c")]))
        );
        assert!(!was_degraded);
    }

    #[test]
    fn derive_relaxed_preserves_phrases_as_single_alternatives() {
        let strict = TextQuery::And(vec![term("a"), phrase("b c")]);
        let (relaxed, was_degraded) = derive_relaxed(&strict);
        assert_eq!(relaxed, Some(TextQuery::Or(vec![term("a"), phrase("b c")])));
        assert!(!was_degraded);
    }

    #[test]
    fn derive_relaxed_drops_top_level_not() {
        // And([a, Not(b)]) -> drop Not -> [a] -> single-term -> None.
        let strict = TextQuery::And(vec![term("a"), TextQuery::Not(Box::new(term("b")))]);
        let (relaxed, was_degraded) = derive_relaxed(&strict);
        assert_eq!(relaxed, None);
        assert!(!was_degraded);

        // And([a, b, Not(c)]) -> drop Not -> [a, b] -> Or([a, b]).
        let strict2 = TextQuery::And(vec![
            term("a"),
            term("b"),
            TextQuery::Not(Box::new(term("c"))),
        ]);
        let (relaxed2, was_degraded2) = derive_relaxed(&strict2);
        assert_eq!(relaxed2, Some(TextQuery::Or(vec![term("a"), term("b")])));
        assert!(!was_degraded2);
    }

    #[test]
    fn derive_relaxed_all_top_level_nots_returns_none() {
        // And([Not(a), Not(b)]) -> drop all Nots -> empty -> None.
        let strict = TextQuery::And(vec![
            TextQuery::Not(Box::new(term("a"))),
            TextQuery::Not(Box::new(term("b"))),
        ]);
        let (relaxed, was_degraded) = derive_relaxed(&strict);
        assert_eq!(relaxed, None);
        assert!(!was_degraded);
    }

    #[test]
    fn derive_relaxed_returns_none_for_or_at_root() {
        let strict = TextQuery::Or(vec![term("a"), term("b")]);
        let (relaxed, was_degraded) = derive_relaxed(&strict);
        assert_eq!(relaxed, None);
        assert!(!was_degraded);
    }

    #[test]
    fn derive_relaxed_returns_none_for_single_term() {
        let (relaxed, was_degraded) = derive_relaxed(&term("budget"));
        assert_eq!(relaxed, None);
        assert!(!was_degraded);

        let (relaxed, was_degraded) = derive_relaxed(&phrase("release notes"));
        assert_eq!(relaxed, None);
        assert!(!was_degraded);
    }

    #[test]
    fn derive_relaxed_caps_at_four_alternatives_and_marks_degraded() {
        let strict = TextQuery::And(vec![term("a"), term("b"), term("c"), term("d"), term("e")]);
        let (relaxed, was_degraded) = derive_relaxed(&strict);
        assert_eq!(
            relaxed,
            Some(TextQuery::Or(vec![
                term("a"),
                term("b"),
                term("c"),
                term("d"),
            ]))
        );
        assert!(was_degraded);
    }

    #[test]
    fn derive_relaxed_cap_preserves_token_order() {
        let strict = TextQuery::And(vec![
            term("alpha"),
            term("bravo"),
            term("charlie"),
            term("delta"),
            term("echo"),
            term("foxtrot"),
        ]);
        let (relaxed, was_degraded) = derive_relaxed(&strict);
        let Some(TextQuery::Or(kept)) = relaxed else {
            panic!("expected Or");
        };
        assert_eq!(
            kept,
            vec![term("alpha"), term("bravo"), term("charlie"), term("delta"),]
        );
        assert!(was_degraded);
    }

    #[test]
    fn derive_relaxed_returns_none_for_nested_and_or_child() {
        // Top-level And containing a nested And is not recursively relaxed in
        // Phase 3.
        let strict = TextQuery::And(vec![term("a"), TextQuery::And(vec![term("b"), term("c")])]);
        let (relaxed, was_degraded) = derive_relaxed(&strict);
        assert_eq!(relaxed, None);
        assert!(!was_degraded);
    }
}
