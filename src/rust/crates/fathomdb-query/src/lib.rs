#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryAst {
    pub raw: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledQuery {
    pub match_expression: String,
}

#[must_use]
pub fn compile_text_query(raw: impl Into<String>) -> CompiledQuery {
    let raw = raw.into();
    let normalized = raw.split_whitespace().filter(|token| !token.is_empty()).collect::<Vec<_>>();
    let match_expression = normalized
        .into_iter()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");

    CompiledQuery { match_expression }
}

#[cfg(test)]
mod tests {
    use super::compile_text_query;

    #[test]
    fn normalizes_whitespace() {
        let compiled = compile_text_query("alpha   beta");
        assert_eq!(compiled.match_expression, "\"alpha\" AND \"beta\"");
    }

    #[test]
    fn escapes_double_quotes_in_tokens() {
        let compiled = compile_text_query("alpha \"beta");
        assert_eq!(compiled.match_expression, "\"alpha\" AND \"\"\"beta\"");
    }
}
