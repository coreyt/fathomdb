#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryAst {
    pub root_kind: String,
    pub steps: Vec<QueryStep>,
    pub final_limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryStep {
    VectorSearch {
        query: String,
        limit: usize,
    },
    TextSearch {
        query: String,
        limit: usize,
    },
    Traverse {
        direction: TraverseDirection,
        label: String,
        max_depth: usize,
    },
    Filter(Predicate),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Predicate {
    LogicalIdEq(String),
    KindEq(String),
    JsonPathEq { path: String, value: ScalarValue },
    SourceRefEq(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScalarValue {
    Text(String),
    Integer(i64),
    Bool(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraverseDirection {
    In,
    Out,
}
