//! Verify the file map, in both directions.
//!
//! The map exists so agents can find their way without ingesting the tree, and
//! that only works while it is trustworthy: the first time a listed path is
//! missing — or a real directory is absent from the map — an agent rationally
//! stops believing it and goes back to enumerating. So mismatches are failures,
//! never warnings.

use std::collections::BTreeSet;
use std::path::Path;

pub fn run(root: &Path) -> Result<(), String> {
    let path = root.join("doc/revision_file_map.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let map: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?;

    let ignored: BTreeSet<String> = map["meta"]["ignore"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str())
                .map(normalize)
                .collect()
        })
        .unwrap_or_default();

    let entry = map["entry"]
        .as_array()
        .ok_or_else(|| "file map has no `entry` array".to_string())?;

    let mut problem: Vec<String> = Vec::new();
    let mut listed: BTreeSet<String> = BTreeSet::new();

    // Forward: everything the map claims exists, does.
    for item in entry {
        let Some(raw) = item["path"].as_str() else {
            problem.push("an entry has no `path`".to_string());
            continue;
        };
        let normalized = normalize(raw);
        if !root.join(&normalized).exists() {
            problem.push(format!("`{raw}` is listed but does not exist"));
        }
        if item["desc"].as_str().is_none_or(str::is_empty) {
            problem.push(format!("`{raw}` has no description"));
        }
        if let Some(description) = item["desc"].as_str()
            && description.chars().count() > 140
        {
            problem.push(format!(
                "`{raw}` description is {} characters (budget is 140)",
                description.chars().count()
            ));
        }
        if !listed.insert(normalized) {
            problem.push(format!("`{raw}` is listed twice"));
        }
    }

    // Backward: everything of consequence is covered. "Of consequence" means
    // every top-level entry and every crate — deeper files are covered by their
    // directory, since the map is directory-granular by design.
    for name in read_names(root)? {
        let normalized = normalize(&name);
        if ignored.contains(&normalized) || covered(&listed, &normalized) {
            continue;
        }
        problem.push(format!(
            "`{name}` exists but is not in the map (list it, or add it to meta.ignore)"
        ));
    }

    let crate_root = root.join("src/rust");
    if crate_root.is_dir() {
        for name in read_names(&crate_root)? {
            let normalized = format!("src/rust/{}", normalize(&name));
            if ignored.contains(&normalized) || covered(&listed, &normalized) {
                continue;
            }
            problem.push(format!("crate `{normalized}` is not in the map"));
        }
    }

    if !problem.is_empty() {
        return Err(format!(
            "file map does not match the tree:\n  {}",
            problem.join("\n  ")
        ));
    }
    println!("file map is current ({} entries)", entry.len());
    Ok(())
}

/// Listed outright, or covered by an entry beneath it — `.github/workflows/`
/// accounts for `.github`, and the crate entries account for `src`.
fn covered(listed: &BTreeSet<String>, path: &str) -> bool {
    listed.contains(path)
        || listed
            .iter()
            .any(|entry| entry.starts_with(&format!("{path}/")))
}

fn read_names(directory: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for item in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        out.push(item.file_name().to_string_lossy().into_owned());
    }
    out.sort();
    Ok(out)
}

/// Trailing slashes are a readability choice in the map, not part of the path.
fn normalize(path: &str) -> String {
    path.trim_end_matches('/').replace('\\', "/")
}
