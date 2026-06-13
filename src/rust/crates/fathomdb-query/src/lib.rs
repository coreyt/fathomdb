use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryAst {
    pub raw: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledQuery {
    pub match_expression: String,
}

/// Function words stripped from the content-OR query (they add false matches
/// under OR semantics without carrying topical signal). Mirrors the IR-C
/// `content-OR` experiment list (`dev/plans/runs/performance-output-and-compare.md`,
/// 2026-06-10b): the smallest list that lifted exploratory recall with no
/// exact_fact cost.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "was", "were", "what", "when", "where", "who", "whom", "which",
    "how", "why", "did", "does", "do", "is", "of", "to", "in", "on", "at", "by", "an", "a", "it",
    "its", "this", "that", "these", "those", "with", "from", "as", "be", "or", "if", "about",
    "into", "over", "than", "then", "they", "them", "their", "you", "your", "we", "our", "i",
];

/// Content tokens of a query: lowercased, split on non-alphanumeric, ≥3 chars,
/// stopwords removed, de-duplicated in first-seen order. These are the OR terms
/// of the compiled MATCH expression. Splitting on non-alphanumeric drops every
/// FTS5 control character (`*`, `"`, `:`, `^`, `(`, `)`, `,`), so the emitted
/// tokens are pure literals — the injection-safety property (AC-038).
#[must_use]
fn content_tokens(raw: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for token in raw.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if token.len() < 3 || stop.contains(token) {
            continue;
        }
        if seen.insert(token.to_string()) {
            out.push(token.to_string());
        }
    }
    out
}

/// Compile a raw query into an FTS5 MATCH expression.
///
/// IR-C (2026-06-10b/c, `performance-output-and-compare.md`): the production
/// arm was an **AND** of every whitespace token, which near-zeroes recall on
/// natural-language questions (every token must be present). The validated
/// recipe is **content-OR** — OR over the content tokens (stopwords stripped) —
/// which any-token-matches and lets `bm25()` rank by overlap, the way the
/// same-dataset BM25 baselines (EnronQA/QAConv) are run.
///
/// All-stopword / symbol-only / sub-3-char queries (no content tokens) fall back
/// to an OR over the raw whitespace tokens as injection-safe quoted phrases, so
/// such a query still searches instead of returning nothing.
#[must_use]
pub fn compile_text_query(raw: impl Into<String>) -> CompiledQuery {
    let raw = raw.into();
    let content = content_tokens(&raw);
    let match_expression = if content.is_empty() {
        raw.split_whitespace()
            .filter(|token| !token.is_empty())
            .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ")
    } else {
        content.into_iter().map(|token| format!("\"{token}\"")).collect::<Vec<_>>().join(" OR ")
    };

    CompiledQuery { match_expression }
}

#[cfg(test)]
mod tests {
    use super::compile_text_query;

    #[test]
    fn content_tokens_are_or_joined() {
        // IR-C content-OR: content tokens OR-joined (was AND).
        let compiled = compile_text_query("alpha   beta");
        assert_eq!(compiled.match_expression, "\"alpha\" OR \"beta\"");
    }

    #[test]
    fn stopwords_and_short_tokens_are_dropped() {
        // "of"/"the" are stopwords; "a" is sub-3-char — only content survives.
        let compiled = compile_text_query("status of the alpha");
        assert_eq!(compiled.match_expression, "\"status\" OR \"alpha\"");
    }

    #[test]
    fn duplicate_content_tokens_collapse_in_order() {
        let compiled = compile_text_query("alpha beta alpha");
        assert_eq!(compiled.match_expression, "\"alpha\" OR \"beta\"");
    }

    #[test]
    fn control_characters_are_stripped_to_literals() {
        // FTS5 control syntax splits into literal content tokens — no operators
        // reach SQLite (AC-038).
        let compiled = compile_text_query("alpha* AND \"beta\" NEAR(gamma)");
        assert_eq!(compiled.match_expression, "\"alpha\" OR \"beta\" OR \"near\" OR \"gamma\"");
    }

    #[test]
    fn all_stopword_query_falls_back_to_raw_or() {
        // No content tokens: OR over the raw whitespace tokens (still searches).
        let compiled = compile_text_query("who are we");
        assert_eq!(compiled.match_expression, "\"who\" OR \"are\" OR \"we\"");
    }

    #[test]
    fn escapes_double_quotes_in_fallback_tokens() {
        // The fallback path keeps injection-safe quote-escaping.
        let compiled = compile_text_query("an \"of");
        assert_eq!(compiled.match_expression, "\"an\" OR \"\"\"of\"");
    }
}
