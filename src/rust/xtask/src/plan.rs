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
//!
//! Two of the checks (misc-05) are not about the viewer at all — they are about
//! references that nothing else verifies. **Filing**: a proposal moves to
//! `doc/completed/` when its plan item completes and lives in `doc/` until then;
//! that getstarted rule was remembered, not enforced, and it slipped — ui-06's
//! proposal sat in `doc/` after the item was done until a hand audit moved it.
//! **Links**: every `doc/…` reference resolves, both the plan's own `links` and
//! every `doc/…` citation buried in a source comment — the second is the one
//! that rots, because moving a proposal breaks comments nowhere near it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

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

    // --- misc-05: references nothing else checks — proposal filing and every
    // documentation link, in the plan and in the source.
    check_filing(root, item, &archived, &mut problem);
    check_links(root, item, &mut problem);

    if problem.is_empty() {
        println!(
            "plan is well formed ({} items, {} log entries); proposals filed and \
             references resolve",
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

/// Enforce the getstarted filing rule: a proposal moves to `doc/completed/`
/// when its plan item completes, and lives in `doc/` until then.
///
/// Ownership is "a plan item links this file", matched by basename so a link
/// with a stale directory still associates. The two directions are deliberately
/// asymmetric. A proposal filed under `completed/` wants at least one owner that
/// is finished — otherwise it was archived too soon. A proposal loose in `doc/`
/// wants at least one owner still live — because a finished item's proposal must
/// be put away, but one a live item still needs stays put even if some other,
/// finished item also cites it. A proposal no item links at all cannot be placed
/// by this rule and is almost certainly orphaned, so it is flagged.
fn check_filing(
    root: &Path,
    items: &[serde_json::Value],
    archived: &[serde_json::Value],
    problem: &mut Vec<String>,
) {
    // Proposal basename -> the completeness of every plan item that links it.
    let mut owner: BTreeMap<String, Vec<bool>> = BTreeMap::new();
    for entry in items.iter().chain(archived) {
        let done = matches!(entry["status"].as_str(), Some("complete" | "archived"));
        for link in entry["links"].as_array().into_iter().flatten() {
            if let Some(doc) = link["doc"].as_str()
                && is_proposal(doc)
            {
                owner
                    .entry(basename(doc).to_string())
                    .or_default()
                    .push(done);
            }
        }
    }

    for (name, in_completed) in proposals_on_disk(root, problem) {
        if let Some(fault) = filing_fault(&name, in_completed, owner.get(&name)) {
            problem.push(fault);
        }
    }
}

/// The filing verdict for one proposal on disk, kept pure so the rule can be
/// tested without a directory. `owners` is the completeness of every plan item
/// that links the file (`None` if none do).
fn filing_fault(name: &str, in_completed: bool, owners: Option<&Vec<bool>>) -> Option<String> {
    match owners {
        None => Some(format!(
            "proposal `{name}` is filed but no plan item links it — filing \
             cannot be verified, and it is almost certainly orphaned"
        )),
        Some(owners) if in_completed && !owners.iter().any(|&done| done) => Some(format!(
            "proposal `{name}` is in doc/completed/ but no plan item that links it is complete"
        )),
        Some(owners) if !in_completed && owners.iter().all(|&done| done) => Some(format!(
            "proposal `{name}` is complete but still in doc/ — a completed item's \
             proposal moves to doc/completed/"
        )),
        Some(_) => None,
    }
}

/// Every documentation reference resolves: the plan's own `links`, and every
/// `doc/…md`|`doc/…json` citation in the source. A broken link is only ever
/// found by opening the thing, which is the same failure mode as the `at`/`ts`
/// key that put "undefined" at the top of the plan page.
fn check_links(root: &Path, items: &[serde_json::Value], problem: &mut Vec<String>) {
    // The plan's links: `doc/<link.doc>` exists.
    for entry in items {
        let id = entry["id"].as_str().unwrap_or("?");
        for link in entry["links"].as_array().into_iter().flatten() {
            if let Some(doc) = link["doc"].as_str()
                && !root.join("doc").join(doc).is_file()
            {
                problem.push(format!("`{id}` links `doc/{doc}`, which does not exist"));
            }
        }
    }

    // Source citations: each `doc/…` path named in a `.rs` file resolves.
    let mut source = Vec::new();
    collect_rs(&root.join("src"), &mut source, problem);
    source.sort();
    for file in &source {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) => {
                problem.push(format!("cannot read {}: {e}", file.display()));
                continue;
            }
        };
        for citation in doc_citations(&text) {
            if !root.join(&citation).is_file() {
                let at = file.strip_prefix(root).unwrap_or(file);
                problem.push(format!(
                    "{} cites `{citation}`, which does not exist",
                    at.display().to_string().replace('\\', "/")
                ));
            }
        }
    }
}

/// A proposal is a `.md` whose name says so; the convention is
/// `revision_<slug>_proposal.md`, but the word alone is enough to recognise one.
fn is_proposal(path: &str) -> bool {
    let name = basename(path);
    name.contains("proposal") && name.ends_with(".md")
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Every proposal on disk, paired with whether it sits under `completed/`. A
/// directory that cannot be read is a problem, not a panic.
fn proposals_on_disk(root: &Path, problem: &mut Vec<String>) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for (relative, in_completed) in [("doc", false), ("doc/completed", true)] {
        let dir = root.join(relative);
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if is_proposal(&name) {
                        out.push((name, in_completed));
                    }
                }
            }
            Err(e) => problem.push(format!("cannot read {}: {e}", dir.display())),
        }
    }
    out.sort();
    out
}

/// Collect every `.rs` under a directory, skipping `target`. A read failure is
/// reported and the walk goes on.
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>, problem: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            problem.push(format!("cannot read {}: {e}", dir.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_rs(&path, out, problem);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `doc/…md` or `doc/…json` path a source file mentions, as it resolves
/// from the repo root. Hand-rolled rather than take a regex dependency for one
/// pattern: find each `doc/` prefix, take the path-shaped run after it, and keep
/// it when it names a doc file. A trailing period in prose is not an extension,
/// so the span is cut at the first real one.
fn doc_citations(text: &str) -> Vec<String> {
    const PREFIX: &str = "doc/";
    let byte = text.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find(PREFIX) {
        let start = from + offset;
        let mut end = start + PREFIX.len();
        while end < byte.len() && is_path_byte(byte[end]) {
            end += 1;
        }
        from = end;
        let span = &text[start..end];
        for extension in [".md", ".json"] {
            if let Some(at) = span.find(extension) {
                out.push(span[..at + extension.len()].to_string());
                break;
            }
        }
    }
    out
}

fn is_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'/' | b'.' | b'-')
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
