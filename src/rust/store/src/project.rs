//! The project: a SQLite database that *is* the model (R-201). There is no
//! unsaved state — every committed gesture is durable the moment it commits, so
//! reopening after an abnormal termination loses nothing completed (R-202).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use rev_core::Command;

use crate::error::StoreError;
use crate::exec;
use crate::journal;
use crate::query;
use crate::schema;

pub struct Project {
    write: Connection,
    read: Connection,
    path: PathBuf,
}

/// A scope in which commands are applied. One gesture is one transaction and
/// one undo unit: it commits whole or not at all, and a panic or error inside
/// the closure rolls it back.
pub struct Gesture<'a> {
    conn: &'a Connection,
    gesture: i64,
}

impl Gesture<'_> {
    /// Apply a command, journal it, and hand back its resolved form (ids and
    /// timestamps filled in).
    pub fn exec(&mut self, command: Command) -> Result<Command, StoreError> {
        let (resolved, inverse) = exec::execute(self.conn, command)?;
        journal::append_command(self.conn, self.gesture, &resolved, &inverse)?;
        Ok(resolved)
    }

    /// Reads *inside* the gesture, which see its uncommitted rows — needed when
    /// one command in a gesture depends on an earlier one.
    pub fn connection(&self) -> &Connection {
        self.conn
    }

    pub fn gesture_number(&self) -> i64 {
        self.gesture
    }
}

impl Project {
    /// Create a new project: schema, then the genesis gesture that seeds meta
    /// and the builtin tunings and scales.
    pub fn create(path: impl AsRef<Path>) -> Result<Project, StoreError> {
        let mut project = Project::create_bare(path)?;
        crate::genesis::seed(&mut project)?;
        Ok(project)
    }

    /// Create a project with schema but no genesis — the target for replay,
    /// which reconstructs even the builtins from history.
    pub fn create_bare(path: impl AsRef<Path>) -> Result<Project, StoreError> {
        let path = path.as_ref().to_path_buf();
        let write = Connection::open(&path)?;
        configure(&write)?;
        schema::create(&write)?;
        let read = open_reader(&path)?;
        Ok(Project { write, read, path })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Project, StoreError> {
        let path = path.as_ref().to_path_buf();
        let write = Connection::open(&path)?;
        configure(&write)?;
        let version = query::meta(&write, schema::META_SCHEMA_VERSION)?
            .ok_or_else(|| StoreError::NotAProject(path.display().to_string()))?
            .parse::<i64>()
            .map_err(|_| StoreError::NotAProject(path.display().to_string()))?;
        if version != schema::SCHEMA_VERSION {
            return Err(StoreError::SchemaVersion {
                found: version,
                expected: schema::SCHEMA_VERSION,
            });
        }
        let read = open_reader(&path)?;
        Ok(Project { write, read, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The read-only connection every query takes.
    ///
    /// Read-only at the SQLite level, not by convention: commands stay the only
    /// writer because this handle physically cannot write. It also cannot see
    /// an in-flight gesture's uncommitted rows, which is correct — the database
    /// is authoritative at commit boundaries.
    pub fn reader(&self) -> &Connection {
        &self.read
    }

    /// Run a gesture. Commits on `Ok`, rolls back on `Err`.
    pub fn gesture<T>(
        &mut self,
        f: impl FnOnce(&mut Gesture) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let tx = self.write.transaction()?;
        let gesture = journal::next_gesture(&tx)?;
        let outcome = {
            let mut scope = Gesture { conn: &tx, gesture };
            f(&mut scope)
        };
        match outcome {
            Ok(value) => {
                tx.commit()?;
                Ok(value)
            }
            Err(error) => {
                tx.rollback()?;
                Err(error)
            }
        }
    }

    /// One command as its own gesture — the common case.
    pub fn apply(&mut self, command: Command) -> Result<Command, StoreError> {
        self.gesture(|g| g.exec(command))
    }

    /// Undo the most recent gesture that is not already undone. Returns false
    /// when there is nothing to undo.
    pub fn undo(&mut self) -> Result<bool, StoreError> {
        let tx = self.write.transaction()?;
        let Some(target) = journal::next_undo(&tx)? else {
            tx.rollback()?;
            return Ok(false);
        };
        let payload = journal::undo_payload(&tx, target)?;
        let gesture = journal::next_gesture(&tx)?;
        for command in payload {
            exec::execute(&tx, command)?;
        }
        journal::append_marker(&tx, gesture, journal::KIND_UNDO, target)?;
        tx.commit()?;
        Ok(true)
    }

    /// Redo the most recently undone gesture, if the redo stack is still live.
    pub fn redo(&mut self) -> Result<bool, StoreError> {
        let tx = self.write.transaction()?;
        let Some(target) = journal::next_redo(&tx)? else {
            tx.rollback()?;
            return Ok(false);
        };
        let payload = journal::redo_payload(&tx, target)?;
        let gesture = journal::next_gesture(&tx)?;
        for command in payload {
            exec::execute(&tx, command)?;
        }
        journal::append_marker(&tx, gesture, journal::KIND_REDO, target)?;
        tx.commit()?;
        Ok(true)
    }

    /// Apply commands without journaling them — replay's path, where history
    /// already exists and must not be duplicated.
    pub(crate) fn apply_unjournaled(
        &mut self,
        command: impl IntoIterator<Item = Command>,
    ) -> Result<(), StoreError> {
        let tx = self.write.transaction()?;
        for item in command {
            exec::execute(&tx, item)?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn configure(conn: &Connection) -> Result<(), StoreError> {
    // WAL gives atomic, power-loss-safe commits and lets the read-only handle
    // work while a gesture is in flight.
    let _mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    register_module(conn)
}

/// `generate_series` is a virtual table module registered per connection, not a
/// built-in — and `v_realized` unrolls loops with it, so every connection that
/// might touch the view needs it, readers included.
fn register_module(conn: &Connection) -> Result<(), StoreError> {
    rusqlite::vtab::series::load_module(conn)?;
    Ok(())
}

fn open_reader(path: &Path) -> Result<Connection, StoreError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    register_module(&conn)?;
    Ok(conn)
}
