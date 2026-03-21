#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchemaVersion(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    pub version: SchemaVersion,
    pub description: &'static str,
    pub sql: &'static str,
}

impl Migration {
    pub const fn new(
        version: SchemaVersion,
        description: &'static str,
        sql: &'static str,
    ) -> Self {
        Self {
            version,
            description,
            sql,
        }
    }
}
