//! Model-state comparison.
//!
//! Renders every model table as sorted, canonical text so two projects can be
//! compared exactly — and, when they differ, so the difference is *readable*
//! rather than a mismatched hash. Floats are compared by their bits, because
//! "identical state" for a materialized tuning means identical frequencies, not
//! nearly-identical ones.
//!
//! History is deliberately excluded: replay reproduces model state, not the
//! journal, which by construction differs (a replayed project's history is the
//! one it was replayed from).

use rusqlite::{Connection, types::ValueRef};

use rev_store::schema::MODEL_TABLE;

/// Canonical text for one table's contents.
pub fn table_text(conn: &Connection, table: &str) -> rusqlite::Result<String> {
    let mut statement = conn.prepare(&format!("SELECT * FROM {table}"))?;
    let column_count = statement.column_count();
    let mut rows = statement.query([])?;
    // Row order is storage order, which is not part of the model's meaning —
    // so render each row, then sort.
    let mut line = Vec::new();
    while let Some(row) = rows.next()? {
        let mut cell = Vec::with_capacity(column_count);
        for index in 0..column_count {
            cell.push(match row.get_ref(index)? {
                ValueRef::Null => "NULL".to_string(),
                ValueRef::Integer(value) => value.to_string(),
                // Bits, not decimal: a materialization that differs in the last
                // place is a different materialization.
                ValueRef::Real(value) => format!("f{:016x}", value.to_bits()),
                ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned(),
                ValueRef::Blob(value) => format!("blob{}", value.len()),
            });
        }
        line.push(cell.join("|"));
    }
    line.sort();
    Ok(line.join("\n"))
}

/// Canonical text for the whole model — every table, in schema order.
pub fn model_text(conn: &Connection) -> rusqlite::Result<String> {
    let mut out = String::new();
    for table in MODEL_TABLE {
        out.push_str("== ");
        out.push_str(table);
        out.push('\n');
        out.push_str(&table_text(conn, table)?);
        out.push('\n');
    }
    Ok(out)
}

/// Assert two connections hold identical model state, naming the first table
/// that differs.
pub fn assert_same_model(left: &Connection, right: &Connection) {
    for table in MODEL_TABLE {
        let a = table_text(left, table).expect("left table");
        let b = table_text(right, table).expect("right table");
        assert_eq!(a, b, "table `{table}` differs");
    }
}
