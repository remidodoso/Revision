//! Screenshot golden masters.
//!
//! Comparison is **bit-identical**, not tolerant — which is affordable only
//! because rendering is CPU-side with bundled fonts and a pinned locale. Tolerances
//! would be the price of a GPU renderer; exactness is what we bought instead.
//!
//! **References are keyed to a toolchain.** A rasterizer or shaper upgrade may
//! legitimately change output. Regenerating is then a deliberate, reviewed act —
//! set `REVISION_BLESS=1` — and never a reflex when CI goes red.

use std::path::{Path, PathBuf};

/// Where screenshot references live: `testdata/ui/`, beside the audio references.
pub fn reference_dir() -> PathBuf {
    // From this crate's manifest: src/rust/testkit -> repo root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../testdata/ui")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("testdata/ui"))
}

/// Compare a rendered PNG against its committed reference.
///
/// A missing reference is written and then **fails**, rather than passing quietly:
/// a master nobody looked at is not a master, and the failure is the prompt to look.
pub fn compare_png(name: &str, actual: &[u8]) -> Result<(), String> {
    let path = reference_dir().join(format!("{name}.png"));
    let bless = std::env::var("REVISION_BLESS").is_ok_and(|v| v == "1");

    if !path.exists() {
        write(&path, actual)?;
        return Err(format!(
            "no reference for `{name}`; wrote {}. Look at it, then re-run.",
            path.display()
        ));
    }

    let expected = std::fs::read(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if expected == actual {
        return Ok(());
    }
    if bless {
        write(&path, actual)?;
        return Ok(());
    }

    // Keep the failing render beside the reference so the two can be compared by
    // eye — a byte count tells you nothing about what moved.
    let actual_path = path.with_extension("actual.png");
    write(&actual_path, actual)?;
    Err(format!(
        "`{name}` differs from its reference ({} bytes vs {}). \
         Wrote {} for comparison. If the change is intended and reviewed, \
         re-run with REVISION_BLESS=1.",
        actual.len(),
        expected.len(),
        actual_path.display()
    ))
}

fn write(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    std::fs::write(path, data).map_err(|e| format!("writing {}: {e}", path.display()))
}
