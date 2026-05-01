#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryAst {
    pub raw: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledQuery {
    pub sql: String,
}

#[must_use]
pub fn compile_text_query(raw: impl Into<String>) -> CompiledQuery {
    let raw = raw.into();
    let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");

    CompiledQuery {
        sql: format!(
            "SELECT document_id FROM search_index WHERE search_index MATCH {:?}",
            normalized
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::compile_text_query;

    #[test]
    fn normalizes_whitespace() {
        let compiled = compile_text_query("alpha   beta");
        assert!(compiled.sql.contains("alpha beta"));
    }
}
