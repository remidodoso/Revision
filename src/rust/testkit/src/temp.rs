//! Throwaway projects on disk.
//!
//! Real files rather than `:memory:`, because the store keeps a second
//! read-only connection to the same database and separate in-memory
//! connections would not see each other.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rev_store::{Project, StoreError};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A project in a temporary directory, removed when dropped.
///
/// The project is held in an `Option` so it can be closed *before* the
/// directory is removed — Windows will not delete a file that still has an open
/// handle, and `reopen` genuinely closes rather than merely adding a second
/// connection.
pub struct TempProject {
    project: Option<Project>,
    directory: PathBuf,
}

impl TempProject {
    /// A new project with genesis seeded — the ordinary case.
    pub fn create() -> Result<TempProject, StoreError> {
        let directory = unique_directory();
        let project = Project::create(directory.join("project.revision"))?;
        Ok(TempProject {
            project: Some(project),
            directory,
        })
    }

    /// A project with schema only — the target replay rebuilds into.
    pub fn create_bare() -> Result<TempProject, StoreError> {
        let directory = unique_directory();
        let project = Project::create_bare(directory.join("project.revision"))?;
        Ok(TempProject {
            project: Some(project),
            directory,
        })
    }

    pub fn project(&self) -> &Project {
        self.project.as_ref().expect("project is open")
    }

    pub fn project_mut(&mut self) -> &mut Project {
        self.project.as_mut().expect("project is open")
    }

    pub fn path(&self) -> PathBuf {
        self.project().path().to_path_buf()
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Close and reopen from disk — how a test asks "did that survive?".
    pub fn reopen(&mut self) -> Result<(), StoreError> {
        let path = self.path();
        self.project = None;
        self.project = Some(Project::open(path)?);
        Ok(())
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        // Close the database before removing its directory.
        self.project = None;
        // Best effort: a leftover temp directory is not worth failing a test.
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn unique_directory() -> PathBuf {
    let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("revision_test_{}_{}", std::process::id(), serial));
    std::fs::create_dir_all(&directory).expect("temp directory");
    directory
}
