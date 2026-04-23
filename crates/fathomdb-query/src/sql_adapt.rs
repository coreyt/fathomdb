//! SQL adaptation helpers for rewriting `compile_query`-generated SQL to match
//! per-kind FTS/vec table layouts at execution time.
//!
//! These helpers operate purely on SQL strings and bind vectors; they do not
//! depend on `rusqlite`. The coordinator is responsible for any `sqlite_master`
//! existence checks and passes the resolved booleans/table names in.

use crate::compile::{BindValue, CompiledQuery};

/// Renumber `SQLite` positional parameters in `sql` after removing the given
/// 1-based parameter numbers from `removed` (sorted ascending).
///
/// Each `?N` in the SQL where `N` is in `removed` is left in place (the caller
/// must have already deleted those references from the SQL). Every `?N` where
/// `N` is greater than any removed parameter is decremented by the count of
/// removed parameters that are less than `N`.
///
/// Example: if `removed = [4]` then `?5` → `?4`, `?6` → `?5`, etc.
/// Example: if `removed = [3, 4]` then `?5` → `?3`, `?6` → `?4`, etc.
pub fn renumber_sql_params(sql: &str, removed: &[usize]) -> String {
    // We walk the string looking for `?` followed by decimal digits and
    // replace the number according to the removal offset.
    let mut result = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'?' {
            // Check if next chars are digits.
            let num_start = i + 1;
            let mut j = num_start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > num_start {
                // Parse the parameter number (1-based).
                let num_str = &sql[num_start..j];
                if let Ok(n) = num_str.parse::<usize>() {
                    // Count how many removed params are < n.
                    let offset = removed.iter().filter(|&&r| r < n).count();
                    result.push('?');
                    result.push_str(&(n - offset).to_string());
                    i = j;
                    continue;
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

/// Strip the property FTS UNION arm from a `compile_query`-generated
/// `DrivingTable::FtsNodes` SQL string.
///
/// When the per-kind `fts_props_<kind>` table does not yet exist the
/// `UNION SELECT ... FROM fts_node_properties ...` arm must be removed so the
/// query degrades to chunk-only results instead of failing with "no such table".
///
/// The SQL structure from `compile_query` (fathomdb-query) is stable:
/// ```text
///                     UNION
///                     SELECT fp.node_logical_id AS logical_id
///                     FROM fts_node_properties fp
///                     ...
///                     WHERE fts_node_properties MATCH ?3
///                       AND fp.kind = ?4
///                 ) u
/// ```
/// We locate the `UNION` that precedes `fts_node_properties` and cut
/// everything from it to the closing `) u`.
pub fn strip_prop_fts_union_arm(sql: &str) -> String {
    // The UNION arm in compile_query-generated FtsNodes SQL has:
    //   - UNION with 24 spaces of indentation
    //   - SELECT fp.node_logical_id with 24 spaces of indentation
    //   - ending at "\n                    ) u" (20 spaces before ") u")
    // Match the UNION that is immediately followed by the property arm.
    let union_marker =
        "                        UNION\n                        SELECT fp.node_logical_id";
    if let Some(start) = sql.find(union_marker) {
        // Find the closing ") u" after the property arm.
        let end_marker = "\n                    ) u";
        if let Some(rel_end) = sql[start..].find(end_marker) {
            let end = start + rel_end;
            // Remove from UNION start to (but not including) the "\n                    ) u" closing.
            return format!("{}{}", &sql[..start], &sql[end..]);
        }
    }
    // Fallback: return unchanged if pattern not found (shouldn't happen).
    sql.to_owned()
}

impl CompiledQuery {
    /// Adapt a `DrivingTable::FtsNodes` compiled query's SQL and binds for the
    /// per-kind FTS property table layout.
    ///
    /// `compile_query` produces SQL that references the legacy global
    /// `fts_node_properties` table. At execution time the coordinator must
    /// decide whether to rewrite that reference to the per-kind
    /// `fts_props_<kind>` table (when it exists) or strip the property FTS
    /// UNION arm entirely (when it does not).
    ///
    /// The caller (the coordinator) performs the `sqlite_master` existence
    /// check and resolves the per-kind table name. `fathomdb-query` has no
    /// `rusqlite` dependency, so the check must remain outside this crate.
    ///
    /// Bind positions in `compile_query`-generated FTS SQL are fixed:
    /// * `?1` = text (chunk FTS)
    /// * `?2` = kind (chunk filter)
    /// * `?3` = text (prop FTS)
    /// * `?4` = kind (prop filter)
    /// * `?5+` = fusable/residual predicates
    ///
    /// When `prop_table_exists` is `true` the helper removes the `fp.kind = ?4`
    /// clause (the per-kind table is already filtered by construction) and
    /// drops `?4` from the bind list.
    ///
    /// When `prop_table_exists` is `false` the helper strips the entire
    /// property FTS UNION arm and drops both `?3` and `?4` from the bind list.
    #[must_use]
    pub fn adapt_fts_for_kind(
        &self,
        prop_table_exists: bool,
        prop_table_name: &str,
    ) -> (String, Vec<BindValue>) {
        let (new_sql, removed_bind_positions) = if prop_table_exists {
            let s = self
                .sql
                .replace("fts_node_properties", prop_table_name)
                .replace("\n                          AND fp.kind = ?4", "");
            (renumber_sql_params(&s, &[4]), vec![3usize])
        } else {
            let s = strip_prop_fts_union_arm(&self.sql);
            (renumber_sql_params(&s, &[3, 4]), vec![2usize, 3])
        };

        let new_binds: Vec<BindValue> = self
            .binds
            .iter()
            .enumerate()
            .filter(|(i, _)| !removed_bind_positions.contains(i))
            .map(|(_, b)| b.clone())
            .collect();

        (new_sql, new_binds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::ShapeHash;
    use crate::plan::{DrivingTable, ExecutionHints};

    fn mk_compiled(sql: &str, binds: Vec<BindValue>) -> CompiledQuery {
        CompiledQuery {
            sql: sql.to_owned(),
            binds,
            shape_hash: ShapeHash(0),
            driving_table: DrivingTable::FtsNodes,
            hints: ExecutionHints {
                recursion_limit: 0,
                hard_limit: 0,
            },
            semantic_search: None,
            raw_vector_search: None,
        }
    }

    #[test]
    fn renumber_shifts_params_past_single_removal() {
        let out = renumber_sql_params("SELECT ?1, ?2, ?4, ?5, ?6", &[4]);
        assert_eq!(out, "SELECT ?1, ?2, ?4, ?4, ?5");
    }

    #[test]
    fn renumber_shifts_params_past_two_removals() {
        let out = renumber_sql_params("SELECT ?1, ?2, ?5, ?6, ?7", &[3, 4]);
        assert_eq!(out, "SELECT ?1, ?2, ?3, ?4, ?5");
    }

    #[test]
    fn strip_removes_union_arm_between_markers() {
        let sql = "\
SELECT c.logical_id AS logical_id
                    FROM fts_node_chunks c
                    WHERE fts_node_chunks MATCH ?1
                      AND c.kind = ?2
                        UNION
                        SELECT fp.node_logical_id AS logical_id
                        FROM fts_node_properties fp
                        WHERE fts_node_properties MATCH ?3
                          AND fp.kind = ?4
                    ) u";
        let out = strip_prop_fts_union_arm(sql);
        assert!(!out.contains("fts_node_properties"));
        assert!(out.contains(") u"));
        assert!(out.contains("fts_node_chunks MATCH ?1"));
    }

    /// When the per-kind property FTS table exists the helper must:
    /// - substitute the per-kind table name for `fts_node_properties`
    /// - remove the `fp.kind = ?4` clause
    /// - renumber params past `?4`
    /// - drop the ?4 bind (index 3 in the 0-based binds vec)
    #[test]
    fn adapt_fts_for_kind_table_exists_rewrites_and_drops_kind_bind() {
        let sql = "\
SELECT ... FROM fts_node_properties fp WHERE fts_node_properties MATCH ?3
                          AND fp.kind = ?4
                          AND extra = ?5";
        let binds = vec![
            BindValue::Text("chunk-text".to_owned()),
            BindValue::Text("Goal".to_owned()),
            BindValue::Text("prop-text".to_owned()),
            BindValue::Text("Goal".to_owned()),
            BindValue::Text("extra".to_owned()),
        ];
        let compiled = mk_compiled(sql, binds);
        let (new_sql, new_binds) = compiled.adapt_fts_for_kind(true, "fts_props_goal");

        assert!(new_sql.contains("fts_props_goal"));
        assert!(!new_sql.contains("fts_node_properties"));
        assert!(!new_sql.contains("AND fp.kind = ?4"));
        // ?5 should have been renumbered to ?4 after removing ?4.
        assert!(new_sql.contains("extra = ?4"));
        assert_eq!(new_binds.len(), 4);
        assert_eq!(new_binds[0], BindValue::Text("chunk-text".to_owned()));
        assert_eq!(new_binds[1], BindValue::Text("Goal".to_owned()));
        assert_eq!(new_binds[2], BindValue::Text("prop-text".to_owned()));
        assert_eq!(new_binds[3], BindValue::Text("extra".to_owned()));
    }

    /// When the per-kind property FTS table does NOT exist the helper must:
    /// - strip the UNION ... `fts_node_properties` ... ) u arm entirely
    /// - renumber params past `?3` and `?4` (both removed)
    /// - drop both the ?3 and ?4 binds (indices 2 and 3)
    #[test]
    fn adapt_fts_for_kind_table_missing_strips_union_and_drops_two_binds() {
        let sql = "\
SELECT c.logical_id AS logical_id
                    FROM fts_node_chunks c
                    WHERE fts_node_chunks MATCH ?1
                      AND c.kind = ?2
                        UNION
                        SELECT fp.node_logical_id AS logical_id
                        FROM fts_node_properties fp
                        WHERE fts_node_properties MATCH ?3
                          AND fp.kind = ?4
                    ) u
                    WHERE extra = ?5";
        let binds = vec![
            BindValue::Text("chunk-text".to_owned()),
            BindValue::Text("Goal".to_owned()),
            BindValue::Text("prop-text".to_owned()),
            BindValue::Text("Goal".to_owned()),
            BindValue::Text("extra".to_owned()),
        ];
        let compiled = mk_compiled(sql, binds);
        let (new_sql, new_binds) = compiled.adapt_fts_for_kind(false, "fts_props_goal");

        assert!(!new_sql.contains("fts_node_properties"));
        assert!(!new_sql.contains("UNION"));
        // ?5 should have been renumbered to ?3 after removing ?3 and ?4.
        assert!(new_sql.contains("extra = ?3"));
        assert_eq!(new_binds.len(), 3);
        assert_eq!(new_binds[0], BindValue::Text("chunk-text".to_owned()));
        assert_eq!(new_binds[1], BindValue::Text("Goal".to_owned()));
        assert_eq!(new_binds[2], BindValue::Text("extra".to_owned()));
    }
}
