//! Generate the browsable schema document.
//!
//! Structure comes from **introspection of a real database**, never from
//! parsing SQL: the command creates a throwaway project (or opens one you name)
//! and asks SQLite what it actually built. That half cannot drift from the code.
//! Prose comes from the annotations in `schema.sql`, where a note sits beside
//! the column it describes — the only place it can, since SQLite has no
//! `COMMENT ON`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{Value, json};

use rev_store::{Project, schema::DDL};

pub fn run(root: &Path, argument: &[&str]) -> Result<(), String> {
    let check = argument.contains(&"--check");
    let target: Option<&&str> = argument.iter().find(|a| !a.starts_with("--"));

    let annotation = Annotation::parse(DDL);
    let document = match target {
        Some(path) => {
            let project =
                Project::open(path).map_err(|e| format!("cannot open project {path}: {e}"))?;
            build(project.reader(), &annotation)?
        }
        None => {
            let probe = probe_project(root)?;
            build(probe.reader(), &annotation)?
        }
    };

    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?
    );
    let destination = root.join("doc/revision_schema.json");

    if check {
        let committed = std::fs::read_to_string(&destination)
            .map_err(|e| format!("cannot read {}: {e}", destination.display()))?;
        if committed.replace("\r\n", "\n") != rendered {
            return Err(format!(
                "{} is out of date — run `cargo xtask schema`",
                destination.display()
            ));
        }
        println!("schema document is current");
        return Ok(());
    }

    std::fs::write(&destination, &rendered)
        .map_err(|e| format!("cannot write {}: {e}", destination.display()))?;
    println!("wrote {}", destination.display());
    Ok(())
}

/// A throwaway project to interrogate, under `target/` so it never litters.
fn probe_project(root: &Path) -> Result<Project, String> {
    let directory = root.join("target/xtask");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path: PathBuf = directory.join("schema-probe.revision");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
    Project::create(&path).map_err(|e| format!("cannot create probe project: {e}"))
}

// ── Annotation extraction ──────────────────────────────────────────────────

#[derive(Default)]
struct Annotation {
    group: Vec<Group>,
    /// Object name → (note, column name → note).
    object: BTreeMap<String, ObjectNote>,
}

struct Group {
    name: String,
    note: String,
    object: Vec<String>,
}

#[derive(Default)]
struct ObjectNote {
    note: String,
    column: BTreeMap<String, String>,
}

impl Annotation {
    /// Walk the DDL, collecting `-- ##` groups, the comments above each object,
    /// and each column's trailing comment plus any comment-only continuation
    /// lines beneath it.
    ///
    /// A line inside a table body is a column definition exactly when its first
    /// token is a lowercase identifier — which the naming standard guarantees,
    /// and which keeps constraint and continuation lines out without a SQL
    /// parser.
    fn parse(ddl: &str) -> Annotation {
        let mut out = Annotation::default();
        let mut pending: Vec<String> = Vec::new();
        let mut collecting_group = false;
        let mut current_object: Option<String> = None;
        let mut last_column: Option<String> = None;
        let mut in_view = false;

        for raw in ddl.lines() {
            let line = raw.trim();

            if line.is_empty() {
                if current_object.is_none() {
                    pending.clear();
                }
                collecting_group = false;
                continue;
            }

            if let Some(heading) = line.strip_prefix("-- ##") {
                out.group.push(Group {
                    name: heading.trim().to_string(),
                    note: String::new(),
                    object: Vec::new(),
                });
                collecting_group = true;
                pending.clear();
                continue;
            }

            if let Some(comment) = comment_of(line) {
                if collecting_group {
                    if let Some(group) = out.group.last_mut() {
                        push_sentence(&mut group.note, comment);
                    }
                } else if let (Some(object), Some(column)) =
                    (current_object.as_ref(), last_column.as_ref())
                {
                    let note = out.object.entry(object.clone()).or_default();
                    push_sentence(note.column.entry(column.clone()).or_default(), comment);
                } else if current_object.is_none() {
                    pending.push(comment.to_string());
                }
                continue;
            }

            collecting_group = false;

            if let Some(name) = object_name(line) {
                in_view = line.starts_with("CREATE VIEW");
                let entry = out.object.entry(name.clone()).or_default();
                entry.note = pending.join(" ");
                pending.clear();
                if let Some(group) = out.group.last_mut() {
                    group.object.push(name.clone());
                }
                current_object = Some(name);
                last_column = None;
                continue;
            }

            if line.starts_with(')') || line.starts_with(';') {
                current_object = None;
                last_column = None;
                in_view = false;
                continue;
            }

            // Inside a table body: a lowercase first token is a column.
            if let Some(object) = current_object.clone() {
                if in_view {
                    continue;
                }
                match column_name(line) {
                    Some(column) => {
                        let note = out.object.entry(object).or_default();
                        let text = comment_after_code(line).unwrap_or_default();
                        note.column.insert(column.clone(), text.to_string());
                        last_column = Some(column);
                    }
                    // A constraint line ends the previous column's notes; a
                    // continuation (REFERENCES, ON …) keeps them going.
                    None => {
                        if is_constraint(line) {
                            last_column = None;
                        } else if let (Some(column), Some(comment)) =
                            (last_column.as_ref(), comment_after_code(line))
                        {
                            let note = out.object.entry(object).or_default();
                            push_sentence(note.column.entry(column.clone()).or_default(), comment);
                        }
                    }
                }
            }
        }
        out
    }
}

fn comment_of(line: &str) -> Option<&str> {
    line.strip_prefix("--").map(str::trim)
}

fn comment_after_code(line: &str) -> Option<&str> {
    line.find("--").map(|at| line[at + 2..].trim())
}

fn object_name(line: &str) -> Option<String> {
    for prefix in ["CREATE TABLE ", "CREATE VIEW "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(
                rest.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches('(')
                    .to_string(),
            );
        }
    }
    None
}

fn column_name(line: &str) -> Option<String> {
    let token = line.split_whitespace().next()?;
    let identifier: String = token
        .chars()
        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
        .collect();
    if !identifier.is_empty()
        && identifier == token
        && token.starts_with(|c: char| c.is_ascii_lowercase())
    {
        Some(identifier)
    } else {
        None
    }
}

fn is_constraint(line: &str) -> bool {
    ["CHECK", "PRIMARY", "FOREIGN", "UNIQUE", "CONSTRAINT"]
        .iter()
        .any(|keyword| line.starts_with(keyword))
}

fn push_sentence(target: &mut String, addition: &str) {
    if !target.is_empty() {
        target.push(' ');
    }
    target.push_str(addition);
}

// ── Introspection ──────────────────────────────────────────────────────────

fn build(conn: &Connection, annotation: &Annotation) -> Result<Value, String> {
    let mut structure = BTreeMap::new();
    let mut statement = conn
        .prepare(
            "SELECT name, type, sql FROM sqlite_master \
             WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(|e| e.to_string())?;
    let listed = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in listed {
        let (name, kind, sql) = row.map_err(|e| e.to_string())?;
        structure.insert(name.clone(), (kind, sql));
    }

    let mut problem: Vec<String> = Vec::new();
    let mut grouped: Vec<Value> = Vec::new();
    let mut placed: Vec<String> = Vec::new();

    for group in &annotation.group {
        let mut object = Vec::new();
        for name in &group.object {
            let Some((kind, sql)) = structure.get(name) else {
                problem.push(format!(
                    "annotated object `{name}` does not exist in the database"
                ));
                continue;
            };
            placed.push(name.clone());
            object.push(describe(
                conn,
                name,
                kind,
                sql.as_deref(),
                annotation,
                &mut problem,
            )?);
        }
        grouped.push(json!({
            "name": group.name,
            "note": group.note,
            "object": object,
        }));
    }

    for name in structure.keys() {
        if !placed.contains(name) {
            problem.push(format!(
                "`{name}` is in no architecture group (add a `-- ##` heading)"
            ));
        }
    }

    if !problem.is_empty() {
        return Err(format!(
            "schema annotation is incomplete:\n  {}",
            problem.join("\n  ")
        ));
    }

    Ok(json!({
        "meta": {
            "generated": true,
            "generator": "cargo xtask schema",
            "source": "src/rust/store/src/schema.sql",
            "schema_version": rev_store::schema::SCHEMA_VERSION,
            "viewer": "revision_schema.html",
        },
        "group": grouped,
    }))
}

fn describe(
    conn: &Connection,
    name: &str,
    kind: &str,
    sql: Option<&str>,
    annotation: &Annotation,
    problem: &mut Vec<String>,
) -> Result<Value, String> {
    let empty = ObjectNote::default();
    let note = annotation.object.get(name).unwrap_or(&empty);

    let mut column = Vec::new();
    // `pragma` feeds rows to a callback rather than returning them, so each
    // introspection collects into a local first.
    let mut info = Vec::new();
    conn.pragma(None, "table_info", name, |row| {
        info.push((
            row.get::<_, String>("name")?,
            row.get::<_, String>("type")?,
            row.get::<_, i64>("notnull")? != 0,
            row.get::<_, Option<String>>("dflt_value")?,
            row.get::<_, i64>("pk")? != 0,
        ));
        Ok(())
    })
    .map_err(|e| format!("table_info({name}): {e}"))?;
    for (column_name, column_type, not_null, default, primary_key) in info {
        let text = note.column.get(&column_name).cloned().unwrap_or_default();
        // Views take their columns from the select, so only tables owe notes.
        if kind == "table" && text.is_empty() {
            problem.push(format!("{name}.{column_name} has no annotation"));
        }
        column.push(json!({
            "name": column_name,
            "type": column_type,
            "not_null": not_null,
            "default": default,
            "primary_key": primary_key,
            "note": text,
        }));
    }

    let mut foreign_key = Vec::new();
    if kind == "table" {
        let mut listed = Vec::new();
        conn.pragma(None, "foreign_key_list", name, |row| {
            listed.push((
                row.get::<_, String>("from")?,
                row.get::<_, String>("table")?,
                row.get::<_, Option<String>>("to")?,
            ));
            Ok(())
        })
        .map_err(|e| format!("foreign_key_list({name}): {e}"))?;
        for (from, table, to) in listed {
            foreign_key.push(json!({
                "column": from,
                "table": table,
                "to": to.unwrap_or_else(|| "id".to_string()),
            }));
        }
    }

    let mut index = Vec::new();
    if kind == "table" {
        let mut listed = Vec::new();
        conn.pragma(None, "index_list", name, |row| {
            listed.push((
                row.get::<_, String>("name")?,
                row.get::<_, i64>("unique")? != 0,
                row.get::<_, String>("origin")?,
                row.get::<_, i64>("partial")? != 0,
            ));
            Ok(())
        })
        .map_err(|e| format!("index_list({name}): {e}"))?;
        for (index_name, unique, origin, partial) in listed {
            let mut column_of: Vec<String> = Vec::new();
            conn.pragma(None, "index_info", index_name.clone(), |row| {
                if let Some(column) = row.get::<_, Option<String>>("name")? {
                    column_of.push(column);
                }
                Ok(())
            })
            .map_err(|e| format!("index_info({index_name}): {e}"))?;
            index.push(json!({
                "name": index_name,
                "unique": unique,
                // 'c' is a CREATE INDEX we wrote; 'u'/'pk' are implied by a
                // UNIQUE or PRIMARY KEY constraint.
                "declared": origin == "c",
                "partial": partial,
                "column": column_of,
            }));
        }
        index.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    }

    Ok(json!({
        "name": name,
        "kind": kind,
        "note": note.note,
        "column": column,
        "foreign_key": foreign_key,
        "index": index,
        // The view's own SQL: it is the model's executable specification, so it
        // is worth reading rather than merely listing.
        "sql": if kind == "view" { sql.map(normalize_sql) } else { None },
    }))
}

/// Strip the comment lines from a view's stored SQL — they are already the
/// object's note, and repeating them in the code block reads as clutter.
fn normalize_sql(sql: &str) -> String {
    sql.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
}
