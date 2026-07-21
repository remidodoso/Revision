-- Revision observation log. Approved at eng-01 §9.4.
--
-- This is *not* the project store. It is one forever-growing file in the OS
-- application-data directory, shared by every run of the application, pruned by
-- size. A session is a column here, not a filename: per-session files litter a
-- disk and — worse — make the cross-session questions ("has this xrun happened
-- before?", "when did this start?") impossible to ask.
--
-- Durability is deliberately weaker than the project store's: `synchronous =
-- NORMAL`, because a log may lose its last few records in a power cut and the
-- project journal may not (R-808). Different files, different guarantees.
--
-- Annotation convention matches the project schema (coding standard,
-- "Bookkeeping"), though this file is not part of the generated schema document:
-- `cargo xtask schema` introspects a project, and this is not one.

-- ## Observation — sessions

-- One row per run of the application. Written at open, before any entry.
CREATE TABLE IF NOT EXISTS session (
  id       INTEGER PRIMARY KEY AUTOINCREMENT,  -- referenced by entry.session_id
  started  INTEGER NOT NULL,                   -- unix microseconds at log open
  version  TEXT    NOT NULL,                   -- crate version of the writing build
  platform TEXT    NOT NULL,                   -- target triple, from the build
  build    TEXT    NOT NULL                    -- "debug" or "release"
);

-- ## Observation — entries

-- The log proper. Append-only in practice; rows leave only by pruning.
CREATE TABLE IF NOT EXISTS entry (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,  -- monotonic; also the ordering
  session_id INTEGER NOT NULL,                   -- which run wrote this
  ts         INTEGER NOT NULL,                   -- unix microseconds, app clock
  creator    TEXT    NOT NULL,                   -- dotted origin: ui.transport, engine.sched
  level      INTEGER NOT NULL,                   -- 0 trace, 1 info, 2 warn, 3 error
  text       TEXT    NOT NULL,                   -- rendered message; prose, not a code
  detail     TEXT,                               -- nullable JSON: structure later, no migration
  keep       INTEGER NOT NULL DEFAULT 0,         -- 1 exempts the row from pruning
  FOREIGN KEY (session_id) REFERENCES session (id)
);

-- Filtering to one run is the viewer's most common query; ordering within it
-- comes free from the primary key, so no index on ts (which is monotonic with
-- id anyway and would only cost insert time).
CREATE INDEX IF NOT EXISTS entry_session_id_i ON entry (session_id);
