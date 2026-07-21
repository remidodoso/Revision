//! Verify the plan, against the shape its viewer expects.
//!
//! The plan is the project's memory, and `revision_plan.html` reads it with no
//! validation at all: a mistyped key renders as the string `undefined` on the
//! page and is invisible until somebody looks. That is exactly how it failed —
//! a `now` entry written with `at` instead of `ts` put "undefined" at the top
//! of the document, and four item notes written as bare strings would have done
//! the same in every tooltip.
//!
//! So the rule is the file map's rule: mismatches are **failures, never
//! warnings**. A memory nobody can trust gets re-derived from scratch, which
//! costs more than the check does.

use std::collections::BTreeSet;
use std::path::Path;

/// The `now` banner is the first thing read and the easiest thing to inflate.
/// It says **where the project stands** — what was last done belongs in an
/// item's notes, and why it was done belongs in a proposal. It grew to nearly
/// five thousand characters of narrative before this ceiling existed.
const NOW_LIMIT: usize = 600;

const STATUS: [&str; 6] = [
    "planned",
    "described",
    "executing",
    "verifying",
    "complete",
    "archived",
];

const KIND: [&str; 3] = ["major", "between", "minor"];

pub fn run(root: &Path) -> Result<(), String> {
    let plan = read(root, "doc/revision_plan.json")?;
    let archive = read(root, "doc/revision_plan_archive.json")?;
    let mut problem: Vec<String> = Vec::new();

    // --- `now`: exactly one entry, short, correctly shaped.
    match plan["now"].as_array() {
        None => problem.push("`now` is not an array".to_string()),
        Some(now) if now.len() != 1 => problem.push(format!(
            "`now` holds {} entries; it holds exactly one, and retired entries \
             move to the archive's `log`",
            now.len()
        )),
        Some(now) => {
            check_entry(&now[0], "now", &mut problem);
            if let Some(text) = now[0]["text"].as_str()
                && text.chars().count() > NOW_LIMIT
            {
                problem.push(format!(
                    "`now` is {} characters; the ceiling is {NOW_LIMIT}. It is a \
                     statement of where the project stands, not a diary entry",
                    text.chars().count()
                ));
            }
        }
    }

    // --- The work log: same shape, and it only ever grows.
    match archive["log"].as_array() {
        None => problem.push("the archive has no `log` array".to_string()),
        Some(log) => {
            for (index, entry) in log.iter().enumerate() {
                check_entry(entry, &format!("log[{index}]"), &mut problem);
            }
            let dated: Vec<&str> = log.iter().filter_map(|e| e["ts"].as_str()).collect();
            if dated.windows(2).any(|pair| pair[0] < pair[1]) {
                problem.push("the log is not newest-first".to_string());
            }
        }
    }

    // --- Items: known vocabulary, resolvable dependencies, shaped notes.
    let item = plan["items"]
        .as_array()
        .ok_or_else(|| "the plan has no `items` array".to_string())?;
    let archived = archive["items"].as_array().cloned().unwrap_or_default();

    let known: BTreeSet<&str> = item
        .iter()
        .chain(archived.iter())
        .filter_map(|it| it["id"].as_str())
        .collect();
    let track: BTreeSet<&str> = plan["tracks"]
        .as_array()
        .map(|list| list.iter().filter_map(|t| t.as_str()).collect())
        .unwrap_or_default();

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for entry in item {
        let Some(id) = entry["id"].as_str() else {
            problem.push("an item has no `id`".to_string());
            continue;
        };
        if !seen.insert(id) {
            problem.push(format!("`{id}` appears twice"));
        }
        for (field, allowed) in [("status", &STATUS[..]), ("kind", &KIND[..])] {
            match entry[field].as_str() {
                Some(value) if allowed.contains(&value) => {}
                Some(value) => problem.push(format!("`{id}` has {field} `{value}`")),
                None => problem.push(format!("`{id}` has no `{field}`")),
            }
        }
        if entry["title"].as_str().is_none_or(str::is_empty) {
            problem.push(format!("`{id}` has no title"));
        }
        match entry["track"].as_str() {
            Some(value) if track.contains(value) => {}
            Some(value) => problem.push(format!("`{id}` is on unknown track `{value}`")),
            None => problem.push(format!("`{id}` has no `track`")),
        }
        for dependency in entry["depends_on"].as_array().into_iter().flatten() {
            match dependency.as_str() {
                Some(value) if known.contains(value) => {}
                Some(value) => {
                    problem.push(format!("`{id}` depends on `{value}`, which is not an item"))
                }
                None => problem.push(format!("`{id}` has a non-string dependency")),
            }
        }
        for (index, note) in entry["notes"].as_array().into_iter().flatten().enumerate() {
            check_entry(note, &format!("{id} note[{index}]"), &mut problem);
        }
        // Completion is the one thing the viewer sorts history by, so a
        // complete item without a date silently vanishes from it.
        let complete = entry["status"].as_str() == Some("complete");
        if complete && entry["completed"].as_str().is_none_or(str::is_empty) {
            problem.push(format!("`{id}` is complete but has no `completed` date"));
        }
        if !complete && entry["completed"].as_str().is_some_and(|d| !d.is_empty()) {
            problem.push(format!("`{id}` has a `completed` date but is not complete"));
        }
    }

    if problem.is_empty() {
        println!(
            "plan is well formed ({} items, {} log entries)",
            item.len(),
            archive["log"].as_array().map_or(0, Vec::len)
        );
        return Ok(());
    }
    let mut message = String::from("the plan does not match the shape its viewer reads:\n");
    for line in &problem {
        message.push_str("  ");
        message.push_str(line);
        message.push('\n');
    }
    Err(message)
}

fn read(root: &Path, relative: &str) -> Result<serde_json::Value, String> {
    let path = root.join(relative);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {e}", path.display()))
}

/// Every dated entry in the plan — `now`, log, item notes — is `{ts, text}`.
/// One shape, checked one way.
fn check_entry(entry: &serde_json::Value, where_: &str, problem: &mut Vec<String>) {
    if !entry.is_object() {
        problem.push(format!(
            "{where_} is not an object; entries are {{ts, text}}"
        ));
        return;
    }
    match entry["ts"].as_str() {
        Some(ts) if is_date(ts) => {}
        Some(ts) => problem.push(format!("{where_} has ts `{ts}`, not YYYY-MM-DD")),
        None => problem.push(format!(
            "{where_} has no `ts` — the viewer renders `undefined`"
        )),
    }
    if entry["text"].as_str().is_none_or(str::is_empty) {
        problem.push(format!("{where_} has no `text`"));
    }
}

fn is_date(value: &str) -> bool {
    let byte = value.as_bytes();
    byte.len() == 10
        && byte[4] == b'-'
        && byte[7] == b'-'
        && byte
            .iter()
            .enumerate()
            .all(|(index, c)| index == 4 || index == 7 || c.is_ascii_digit())
}

#[cfg(test)]
mod test;
