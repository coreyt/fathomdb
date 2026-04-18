//! Pure SQL string manipulation helpers for adapting FTS query SQL at runtime.
//!
//! These functions operate only on the SQL string (and parameter index lists);
//! they carry no runtime state and require no database access.

/// Renumber `SQLite` positional parameters (`?N`) in `sql` after the parameters
/// at the 1-based positions in `removed` have been dropped from the bind list.
///
/// For each `?N` token found in the SQL, the function counts how many removed
/// parameter positions are strictly less than `N` and subtracts that count from
/// `N`.
#[must_use]
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
#[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- renumber_sql_params ---

    #[test]
    fn renumber_remove_middle_param() {
        // ?1 stays ?1, ?2 is removed, ?3 becomes ?2, ?4 becomes ?3
        let sql = "SELECT * FROM t WHERE a = ?1 AND b = ?3 AND c = ?4";
        let result = renumber_sql_params(sql, &[2]);
        assert_eq!(result, "SELECT * FROM t WHERE a = ?1 AND b = ?2 AND c = ?3");
    }

    #[test]
    fn renumber_remove_last_param() {
        // ?1 and ?2 unchanged; ?3 is removed (not present in SQL, just skip)
        let sql = "SELECT * FROM t WHERE a = ?1 AND b = ?2";
        let result = renumber_sql_params(sql, &[3]);
        assert_eq!(result, "SELECT * FROM t WHERE a = ?1 AND b = ?2");
    }

    #[test]
    fn renumber_remove_nothing() {
        let sql = "SELECT * FROM t WHERE a = ?1 AND b = ?2 AND c = ?3";
        let result = renumber_sql_params(sql, &[]);
        assert_eq!(result, "SELECT * FROM t WHERE a = ?1 AND b = ?2 AND c = ?3");
    }

    #[test]
    fn renumber_remove_multiple_params() {
        // Remove ?2 and ?4 (0-based representation — but these are 1-based param numbers
        // and the function takes 1-based removed positions).
        // ?1 stays ?1, ?2 removed, ?3 becomes ?2, ?4 removed, ?5 becomes ?3
        let sql = "WHERE a=?1 AND b=?3 AND c=?5";
        let result = renumber_sql_params(sql, &[2, 4]);
        assert_eq!(result, "WHERE a=?1 AND b=?2 AND c=?3");
    }

    // --- strip_prop_fts_union_arm ---

    const SQL_WITH_UNION: &str = "\
SELECT u.logical_id FROM (
                    SELECT fc.node_logical_id AS logical_id
                    FROM fts_node_chunks fc
                    WHERE fts_node_chunks MATCH ?1
                      AND fc.kind = ?2
                        UNION
                        SELECT fp.node_logical_id AS logical_id
                        FROM fts_node_properties fp
                        WHERE fts_node_properties MATCH ?3
                          AND fp.kind = ?4
                    ) u";

    #[test]
    fn strip_union_arm_present() {
        let result = strip_prop_fts_union_arm(SQL_WITH_UNION);
        assert!(
            !result.contains("fts_node_properties"),
            "prop FTS arm should be stripped"
        );
        assert!(
            result.contains("fts_node_chunks"),
            "chunk FTS arm should remain"
        );
        assert!(result.contains("?1"), "?1 should survive");
        assert!(result.contains("?2"), "?2 should survive");
        assert!(!result.contains("?3"), "?3 should be removed with the arm");
        assert!(!result.contains("?4"), "?4 should be removed with the arm");
    }

    #[test]
    fn strip_union_arm_absent_unchanged() {
        let sql = "SELECT * FROM fts_node_chunks WHERE fts_node_chunks MATCH ?1 AND kind = ?2";
        let result = strip_prop_fts_union_arm(sql);
        assert_eq!(
            result, sql,
            "SQL without union arm should be returned unchanged"
        );
    }
}
