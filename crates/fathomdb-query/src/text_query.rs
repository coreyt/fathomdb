/// A constrained full-text query representation for `FathomDB`'s safe search API.
///
/// `TextQuery` models the subset of boolean search supported by
/// [`QueryBuilder::text_search`](crate::QueryBuilder::text_search):
/// literal terms, quoted phrases, uppercase `OR`, uppercase `NOT`, and
/// implicit `AND` by adjacency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextQuery {
    /// An empty query.
    Empty,
    /// A literal search term.
    Term(String),
    /// A literal quoted phrase.
    Phrase(String),
    /// A negated child query.
    Not(Box<TextQuery>),
    /// A conjunction of child queries.
    And(Vec<TextQuery>),
    /// A disjunction of child queries.
    Or(Vec<TextQuery>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Phrase(String),
}

impl TextQuery {
    /// Parse raw user or agent input into `FathomDB`'s supported text-query subset.
    ///
    /// Parsing is intentionally forgiving. Only exact uppercase `OR` and `NOT`
    /// tokens are treated as operators; unsupported or malformed syntax is
    /// downgraded to literal terms instead of being passed through as raw FTS5.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        let tokens = tokenize(raw);
        if tokens.is_empty() {
            return Self::Empty;
        }

        let mut groups = Vec::new();
        let mut current = Vec::new();
        let mut index = 0;

        while index < tokens.len() {
            if is_or_token(&tokens[index]) {
                let can_split = !current.is_empty() && can_start_or_clause(&tokens, index + 1);
                if can_split {
                    groups.push(normalize_and(current));
                    current = Vec::new();
                } else {
                    current.push(Self::Term("OR".to_owned()));
                }
                index += 1;
                continue;
            }

            let (node, next) =
                parse_atom_or_literal(&tokens, index, can_negate_from_current(&current));
            current.push(node);
            index = next;
        }

        if !current.is_empty() {
            groups.push(normalize_and(current));
        }

        match groups.len() {
            0 => Self::Empty,
            1 => groups.into_iter().next().unwrap_or(Self::Empty),
            _ => Self::Or(groups),
        }
    }
}

/// Render a [`TextQuery`] as an FTS5-safe `MATCH` expression.
///
/// The renderer is the only place that emits FTS5 control syntax. All literal
/// terms and phrases are double-quoted and escaped, while only supported
/// operators (`OR`, `NOT`, and implicit `AND`) are emitted as control syntax.
#[must_use]
pub fn render_text_query_fts5(query: &TextQuery) -> String {
    render_with_grouping(query, false)
}

fn render_with_grouping(query: &TextQuery, parenthesize: bool) -> String {
    match query {
        TextQuery::Empty => String::new(),
        TextQuery::Term(term) | TextQuery::Phrase(term) => quote_fts5_literal(term),
        TextQuery::Not(child) => {
            let rendered = render_with_grouping(child, true);
            format!("NOT {rendered}")
        }
        TextQuery::And(children) => {
            let rendered = children
                .iter()
                .map(|child| render_with_grouping(child, matches!(child, TextQuery::Or(_))))
                .collect::<Vec<_>>()
                .join(" ");
            if parenthesize && children.len() > 1 {
                format!("({rendered})")
            } else {
                rendered
            }
        }
        TextQuery::Or(children) => {
            let rendered = children
                .iter()
                .map(|child| render_with_grouping(child, matches!(child, TextQuery::And(_))))
                .collect::<Vec<_>>()
                .join(" OR ");
            if parenthesize && children.len() > 1 {
                format!("({rendered})")
            } else {
                rendered
            }
        }
    }
}

fn quote_fts5_literal(raw: &str) -> String {
    let escaped = raw.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn tokenize(raw: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }

        if chars[index] == '"' {
            let start = index + 1;
            let mut end = start;
            while end < chars.len() && chars[end] != '"' {
                end += 1;
            }
            if end < chars.len() {
                let phrase: String = chars[start..end].iter().collect();
                tokens.push(Token::Phrase(phrase));
                index = end + 1;
                continue;
            }
        }

        let start = index;
        while index < chars.len() && !chars[index].is_whitespace() {
            index += 1;
        }
        let word: String = chars[start..index].iter().collect();
        tokens.push(Token::Word(word));
    }

    tokens
}

fn is_or_token(token: &Token) -> bool {
    matches!(token, Token::Word(word) if word == "OR")
}

fn can_start_or_clause(tokens: &[Token], index: usize) -> bool {
    match tokens.get(index) {
        Some(Token::Phrase(_)) => true,
        Some(Token::Word(word)) => word != "OR" && word != "NOT",
        None => false,
    }
}

fn can_negate_from_current(current: &[TextQuery]) -> bool {
    match current.last() {
        Some(TextQuery::Phrase(_)) => true,
        Some(TextQuery::Term(term)) => term != "OR" && term != "AND" && term != "NOT",
        _ => false,
    }
}

fn parse_atom_or_literal(tokens: &[Token], index: usize, can_negate: bool) -> (TextQuery, usize) {
    match tokens.get(index) {
        Some(Token::Phrase(phrase)) => (TextQuery::Phrase(phrase.clone()), index + 1),
        Some(Token::Word(word)) if word == "NOT" => {
            if can_negate {
                match tokens.get(index + 1) {
                    Some(Token::Phrase(phrase)) => (
                        TextQuery::Not(Box::new(TextQuery::Phrase(phrase.clone()))),
                        index + 2,
                    ),
                    Some(Token::Word(next)) if next != "OR" && next != "NOT" => (
                        TextQuery::Not(Box::new(TextQuery::Term(next.clone()))),
                        index + 2,
                    ),
                    _ => (TextQuery::Term("NOT".to_owned()), index + 1),
                }
            } else {
                (TextQuery::Term("NOT".to_owned()), index + 1)
            }
        }
        Some(Token::Word(word)) => (TextQuery::Term(word.clone()), index + 1),
        None => (TextQuery::Empty, index),
    }
}

fn normalize_and(mut nodes: Vec<TextQuery>) -> TextQuery {
    match nodes.len() {
        0 => TextQuery::Empty,
        1 => nodes.pop().unwrap_or(TextQuery::Empty),
        _ => TextQuery::And(nodes),
    }
}

#[cfg(test)]
mod tests {
    use super::{TextQuery, render_text_query_fts5};

    #[test]
    fn parse_empty_query() {
        assert_eq!(TextQuery::parse(""), TextQuery::Empty);
        assert_eq!(TextQuery::parse("   "), TextQuery::Empty);
    }

    #[test]
    fn parse_plain_terms_as_implicit_and() {
        assert_eq!(
            TextQuery::parse("budget meeting"),
            TextQuery::And(vec![
                TextQuery::Term("budget".into()),
                TextQuery::Term("meeting".into()),
            ])
        );
    }

    #[test]
    fn parse_phrase() {
        assert_eq!(
            TextQuery::parse("\"release notes\""),
            TextQuery::Phrase("release notes".into())
        );
    }

    #[test]
    fn parse_or_operator() {
        assert_eq!(
            TextQuery::parse("ship OR docs"),
            TextQuery::Or(vec![
                TextQuery::Term("ship".into()),
                TextQuery::Term("docs".into()),
            ])
        );
    }

    #[test]
    fn parse_not_operator() {
        assert_eq!(
            TextQuery::parse("ship NOT blocked"),
            TextQuery::And(vec![
                TextQuery::Term("ship".into()),
                TextQuery::Not(Box::new(TextQuery::Term("blocked".into()))),
            ])
        );
    }

    #[test]
    fn parse_leading_not_as_literal() {
        assert_eq!(
            TextQuery::parse("NOT blocked"),
            TextQuery::And(vec![
                TextQuery::Term("NOT".into()),
                TextQuery::Term("blocked".into()),
            ])
        );
    }

    #[test]
    fn parse_not_after_or_as_literal() {
        assert_eq!(
            TextQuery::parse("ship OR NOT blocked"),
            TextQuery::And(vec![
                TextQuery::Term("ship".into()),
                TextQuery::Term("OR".into()),
                TextQuery::Term("NOT".into()),
                TextQuery::Term("blocked".into()),
            ])
        );
    }

    #[test]
    fn parse_lowercase_or_as_literal() {
        assert_eq!(
            TextQuery::parse("ship or docs"),
            TextQuery::And(vec![
                TextQuery::Term("ship".into()),
                TextQuery::Term("or".into()),
                TextQuery::Term("docs".into()),
            ])
        );
    }

    #[test]
    fn parse_lowercase_not_as_literal() {
        assert_eq!(
            TextQuery::parse("not a ship"),
            TextQuery::And(vec![
                TextQuery::Term("not".into()),
                TextQuery::Term("a".into()),
                TextQuery::Term("ship".into()),
            ])
        );
    }

    #[test]
    fn parse_trailing_or_as_literal() {
        assert_eq!(
            TextQuery::parse("ship OR"),
            TextQuery::And(vec![
                TextQuery::Term("ship".into()),
                TextQuery::Term("OR".into()),
            ])
        );
    }

    #[test]
    fn parse_apostrophe_as_literal_term() {
        assert_eq!(
            TextQuery::parse("User's name"),
            TextQuery::And(vec![
                TextQuery::Term("User's".into()),
                TextQuery::Term("name".into()),
            ])
        );
    }

    #[test]
    fn parse_unsupported_column_filter_as_literal() {
        assert_eq!(
            TextQuery::parse("col:value"),
            TextQuery::Term("col:value".into())
        );
    }

    #[test]
    fn parse_unsupported_prefix_as_literal() {
        assert_eq!(
            TextQuery::parse("prefix*"),
            TextQuery::Term("prefix*".into())
        );
    }

    #[test]
    fn parse_near_as_literal() {
        assert_eq!(
            TextQuery::parse("a NEAR b"),
            TextQuery::And(vec![
                TextQuery::Term("a".into()),
                TextQuery::Term("NEAR".into()),
                TextQuery::Term("b".into()),
            ])
        );
    }

    #[test]
    fn parse_explicit_and_as_literal() {
        assert_eq!(
            TextQuery::parse("cats AND dogs OR fish"),
            TextQuery::Or(vec![
                TextQuery::And(vec![
                    TextQuery::Term("cats".into()),
                    TextQuery::Term("AND".into()),
                    TextQuery::Term("dogs".into()),
                ]),
                TextQuery::Term("fish".into()),
            ])
        );
    }

    #[test]
    fn render_term_query() {
        assert_eq!(
            render_text_query_fts5(&TextQuery::Term("budget".into())),
            "\"budget\""
        );
    }

    #[test]
    fn render_phrase_query() {
        assert_eq!(
            render_text_query_fts5(&TextQuery::Phrase("release notes".into())),
            "\"release notes\""
        );
    }

    #[test]
    fn render_or_query() {
        assert_eq!(
            render_text_query_fts5(&TextQuery::Or(vec![
                TextQuery::Term("ship".into()),
                TextQuery::Term("docs".into()),
            ])),
            "\"ship\" OR \"docs\""
        );
    }

    #[test]
    fn render_not_query() {
        assert_eq!(
            render_text_query_fts5(&TextQuery::And(vec![
                TextQuery::Term("ship".into()),
                TextQuery::Not(Box::new(TextQuery::Term("blocked".into()))),
            ])),
            "\"ship\" NOT \"blocked\""
        );
    }

    #[test]
    fn render_escapes_embedded_quotes() {
        assert_eq!(
            render_text_query_fts5(&TextQuery::Term("say \"hello\"".into())),
            "\"say \"\"hello\"\"\""
        );
    }

    #[test]
    fn render_leading_not_literalized_parse_safely() {
        assert_eq!(
            render_text_query_fts5(&TextQuery::parse("NOT blocked")),
            "\"NOT\" \"blocked\""
        );
    }

    #[test]
    fn render_lowercase_not_as_literal_terms() {
        assert_eq!(
            render_text_query_fts5(&TextQuery::parse("not a ship")),
            "\"not\" \"a\" \"ship\""
        );
    }
}
