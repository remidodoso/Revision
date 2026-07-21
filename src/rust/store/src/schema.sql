-- Revision project schema. Approved at core-01.
--
-- This file is the model surface itself (§6g): built-in editors are clients of
-- it with no privileged back door, so the tables *are* the public contract.
-- Column names live here and in the query catalog, nowhere else.
--
-- Annotations follow a convention `cargo xtask schema` extracts into the
-- browsable schema document: `-- ##` opens an architecture group, comments
-- above an object are its note, and a trailing comment on a column line is that
-- column's note (continued by comment-only lines beneath it).
--
-- Model tables use AUTOINCREMENT rather than plain INTEGER PRIMARY KEY because
-- plain row ids are reused: delete the highest row and the next insert takes
-- its id back. Undo deletes rows, so a later create would claim a freed id and
-- redoing the undone gesture — which reinserts at its original explicit id —
-- would collide. The resolved-id discipline rests on ids never being recycled.

-- ## Project — settings
-- One row per setting. Small, and deliberately schemaless: these are knobs, not
-- material.

-- Project-wide settings: schema version, resolution, default tuning, root phrase.
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,  -- setting name; see the META_* constants in schema.rs
  value TEXT NOT NULL      -- always text, parsed by the reader that cares
);

-- ## Tuning — definition, binder, frozen result
-- The three-level stack (R-501..R-506). The definition side stays exact: rules
-- remain rules, ratios remain integer pairs. Materialization compiles a
-- definition into a frozen table of frequencies, and *that* is playback truth —
-- persisted, so a project sounds identical years later no matter what the
-- builder has since learned. Everything downstream resolves through one
-- materialization, which is the funnel a dynamic tuning (R-515) would use.

-- A tuning definition: everything determining the note-number-to-frequency map.
CREATE TABLE tuning (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,  -- never reused; redo reinserts at this exact id
  name             TEXT NOT NULL UNIQUE,               -- user-facing name, e.g. "16-ET"
  description      TEXT,                               -- prose; NULL on a derived row inherits its parent's
  kind             TEXT NOT NULL CHECK (kind IN ('equal','table')),
                                                       -- 'equal' is the only rule kept in the schema, because
                                                       -- its steps are irrational and cannot be stored exactly;
                                                       -- everything else materializes into tuning_note rows
  period_num       INTEGER,                            -- interval of equivalence as an exact ratio: 2/1 octave,
  period_den       INTEGER,                            --   3/1 Bohlen-Pierce; both NULL means aperiodic (R-502)
  note_per_period  INTEGER,                            -- the modulus pitch-class logic uses; NULL when aperiodic
  anchor_note      INTEGER NOT NULL,                   -- the note bound to anchor_freq (R-503); builtins use 60
  anchor_freq      REAL NOT NULL,                      -- middle C for every builtin, so switching a phrase's
                                                       --   tuning keeps its home pitch
  note_min         INTEGER,                            -- materialization domain for kinds whose rule is unbounded;
  note_max         INTEGER,                            --   NULL for an aperiodic table, whose rows are its domain
  naming           TEXT,                               -- R-508 scheme: letter, hex, near12; presentation only
  origin           TEXT,                               -- provenance (R-413): builtin, generated, derived, user
  seed             TEXT,                               -- JSON recipe: generator name, parameters, seed
  parent_tuning_id INTEGER REFERENCES tuning(id),      -- lineage for derived tunings, e.g. re-rooting (R-517 open)
  extra            TEXT NOT NULL DEFAULT '{}',         -- JSON escape valve; a field graduates to a column when it
                                                       --   earns an index or a CHECK
  CHECK ((period_num IS NULL) = (period_den IS NULL)),
  CHECK ((period_num IS NULL) = (note_per_period IS NULL))
);

-- Per-note data for a table tuning: one canonical period when periodic, the
-- whole domain when not. Exact by construction (R-504).
CREATE TABLE tuning_note (
  tuning_id   INTEGER NOT NULL REFERENCES tuning(id),  -- the tuning these notes define
  note_number INTEGER NOT NULL,                        -- absolute; periodic tunings cover [anchor, anchor + N)
  ratio_num   INTEGER,                                 -- exact ratio from the anchor — a just fifth is 3/2,
  ratio_den   INTEGER,                                 --   never 701.955 cents
  freq        REAL,                                    -- or a direct frequency: "this frequency is this note",
                                                       --   for measured instruments and hand-assigned pitches
  PRIMARY KEY (tuning_id, note_number),
  CHECK ((ratio_num IS NULL) = (ratio_den IS NULL)),
  CHECK ((ratio_num IS NOT NULL) <> (freq IS NOT NULL))
) WITHOUT ROWID;

-- The binder: names one materialization of one tuning, and nothing else.
-- Deliberately pure — every column added here is a column every future
-- dynamic-tuning mechanism must reckon with.
CREATE TABLE materialized_tuning_instance (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,        -- what everything downstream resolves through
  tuning_id INTEGER NOT NULL REFERENCES tuning(id),   -- the definition this compiled from
  ts        INTEGER NOT NULL                          -- epoch milliseconds; carried in the resolved command so
                                                      --   replay reproduces it rather than re-stamping
);

-- The frozen result: playback truth. Uniform across every kind, so consumers
-- never branch on how a tuning was defined.
CREATE TABLE materialized_tuning (
  materialized_tuning_instance_id INTEGER NOT NULL
    REFERENCES materialized_tuning_instance(id),  -- which materialization this row belongs to
  note_number INTEGER NOT NULL,                   -- signed; dense tunings run well below the anchor
  freq        REAL NOT NULL,                      -- hertz, strictly increasing with note number — the invariant
                                                  --   nearest-note search rests on
  PRIMARY KEY (materialized_tuning_instance_id, note_number)
) WITHOUT ROWID;

-- ## Scale — shapes, not keys
-- A scale stores a shape and never a rooted result: the mask is relative to the
-- tonic and the root is supplied at the use site, so "major" is one row serving
-- every key and every 12-note tuning. Applicability is by modulus and is the
-- only relationship enforced; whether a mask makes musical sense over a given
-- tuning and root is idiomatic fit — advisory, never prohibited (R-509/R-510).

-- A named subset of pitch classes, or of note numbers when aperiodic.
CREATE TABLE scale (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,   -- never reused, so an undone create can be redone
  name            TEXT NOT NULL,                       -- e.g. "Mavila (7)"
  description     TEXT,                                -- prose; what the shape is and where it comes from
  note_per_period INTEGER,                             -- the modulus a periodic mask belongs to: its structural
                                                       --   parent, so one mask serves 12-ET and just intonation
  tuning_id       INTEGER REFERENCES tuning(id),       -- aperiodic scales only: absolute subsets belong to one
                                                       --   tuning. Exactly one of this and note_per_period is set
  mask            TEXT NOT NULL,                       -- JSON array: root-relative offsets when periodic,
                                                       --   absolute note numbers when not
  origin          TEXT,                                -- provenance (R-413), as for tunings and phrases
  seed            TEXT,                                -- JSON recipe, for generated scales
  parent_scale_id INTEGER REFERENCES scale(id),        -- lineage, e.g. a mode recorded against the mask it rotates
  extra           TEXT NOT NULL DEFAULT '{}',          -- JSON escape valve
  CHECK ((note_per_period IS NULL) <> (tuning_id IS NULL))
);

-- ## Material — phrases, tracks, events, instances
-- The note model (R-001): a phrase is the unit of material, an instance places
-- it in time with its own non-destructive play parameters, and editing a phrase
-- affects every instance of it. There is one kind of track (R-425), holding
-- both direct events and instances.

-- A named container of events: the reusable unit of material (R-401).
CREATE TABLE phrase (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,  -- never reused; instances reference it, undo restores it
  name             TEXT NOT NULL,                      -- user-facing; the library is browsed by it
  length_tick      INTEGER NOT NULL CHECK (length_tick >= 0),
                                                       -- the WINDOW: gates onsets and sets the loop stride.
                                                       --   Events at or beyond it are dormant material —
                                                       --   retained, silent — so narrowing is a non-destructive
                                                       --   mute and widening reveals what was always there
  tuning_id        INTEGER REFERENCES tuning(id),      -- the tuning its note numbers are read in (R-418);
                                                       --   NULL falls back to the project default
  scale_id         INTEGER REFERENCES scale(id),       -- NULL is chromatic — there is no chromatic row (R-510)
  root             INTEGER NOT NULL DEFAULT 0,         -- the pitch class the mask is rooted on; overall root
                                                       --   semantics are still open (R-517)
  origin           TEXT,                               -- recorded, generated, derived (R-413)
  seed             TEXT,                               -- JSON: seed, generator version, parameters
  parent_phrase_id INTEGER REFERENCES phrase(id),      -- the material this was derived from
  extra            TEXT NOT NULL DEFAULT '{}'          -- JSON escape valve; meter map and instrument binding
                                                       --   live here until they earn columns (R-414)
);

-- An ordered container of events and instances (R-406). One kind only (R-425).
CREATE TABLE track (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,       -- never reused
  phrase_id INTEGER NOT NULL REFERENCES phrase(id),  -- the root phrase that owns it; multi-track sub-phrases are
                                                     --   deferred, not foreclosed
  name      TEXT NOT NULL,                           -- user-facing track name
  ord       INTEGER NOT NULL,                        -- sibling order
  extra     TEXT NOT NULL DEFAULT '{}'               -- JSON escape valve
);

-- One event: a note today, and a vocabulary open to controllers, articulation
-- and audio without a schema change (R-402).
CREATE TABLE event (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,  -- never reused; an undone removal restores rows at their ids
  phrase_id   INTEGER REFERENCES phrase(id),  -- container: inside a phrase, XOR
  track_id    INTEGER REFERENCES track(id),   --   directly on a track (R-406)
  kind        TEXT NOT NULL DEFAULT 'note',   -- open vocabulary: note, cc, articulation, audio (R-402)
  at_tick     INTEGER NOT NULL CHECK (at_tick >= 0),
                                              -- position in ticks at 5040 per quarter (R-003); seconds are
                                              --   derived only at the engine boundary
  dur_tick    INTEGER NOT NULL DEFAULT 0 CHECK (dur_tick >= 0),
                                              -- a note crossing the window edge rings out rather than being
                                              --   truncated at every loop seam
  note_number INTEGER,                        -- the pitch datum (R-002): a signed position in a tuning, not a
                                              --   frequency and not a MIDI note. NULL for unpitched kinds
  velocity    INTEGER,                        -- 0..65535, the MIDI 2.0 domain; 7-bit values are translated at
                                              --   the MIDI boundary and the UI presents 0..127
  extra       TEXT NOT NULL DEFAULT '{}',     -- JSON escape valve; per-kind fields live here
  CHECK ((phrase_id IS NULL) <> (track_id IS NULL))
);
CREATE INDEX event_phrase_id_at_tick_i ON event(phrase_id, at_tick) WHERE phrase_id IS NOT NULL;
CREATE INDEX event_track_id_at_tick_i  ON event(track_id,  at_tick) WHERE track_id  IS NOT NULL;

-- A placement of a phrase in time, carrying its own play parameters (R-404/405).
-- All instances share the referenced material: edit the phrase and every
-- instance follows.
CREATE TABLE phrase_instance (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
                                     -- never reused
  phrase_id        INTEGER NOT NULL REFERENCES phrase(id),
                                     -- the material being referenced
  track_id         INTEGER REFERENCES track(id),
                                     -- container: on a track, XOR
  parent_phrase_id INTEGER REFERENCES phrase(id),
                                     --   nested inside a parent phrase (R-407). Cycles are rejected in core
  at_tick          INTEGER NOT NULL CHECK (at_tick >= 0),
                                     -- where the instance starts in its container's time
  offset_tick      INTEGER NOT NULL DEFAULT 0 CHECK (offset_tick >= 0),
                                     -- where the window starts *within the material* — Vision's "play bars
                                     --   8 to 16 of 32"
  length_tick      INTEGER,          -- the instance's own length, independent of the phrase's (R-405);
                                     --   NULL means the natural extent of loop_count iterations
  loop_count       INTEGER NOT NULL DEFAULT 1 CHECK (loop_count >= 1),
                                     -- repetitions; the stride is the phrase's window length
  transpose        INTEGER NOT NULL DEFAULT 0,
                                     -- chromatic transpose in note numbers, read in the material's tuning
                                     --   (R-423). Degree transposition is a separate, scale-relative verb
  mute             INTEGER NOT NULL DEFAULT 0 CHECK (mute IN (0,1)),
                                     -- non-destructive silence
  extra            TEXT NOT NULL DEFAULT '{}',
                                     -- JSON escape valve; articulation template and timbre arc land here
  CHECK ((track_id IS NULL) <> (parent_phrase_id IS NULL))
);
CREATE INDEX phrase_instance_track_id_at_tick_i
  ON phrase_instance(track_id, at_tick) WHERE track_id IS NOT NULL;
CREATE INDEX phrase_instance_phrase_id_i ON phrase_instance(phrase_id);

-- One point in a phrase's tempo map.
CREATE TABLE tempo_point (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,       -- never reused
  phrase_id        INTEGER NOT NULL REFERENCES phrase(id),  -- the phrase whose map this belongs to (R-414)
  at_tick          INTEGER NOT NULL CHECK (at_tick >= 0),   -- where the tempo takes effect
  usec_per_quarter INTEGER NOT NULL CHECK (usec_per_quarter > 0)
                                                            -- microseconds per quarter note: MIDI-exact and
                                                            --   integral, so the model never accumulates float
                                                            --   drift. Seconds are derived at the engine only
);

-- ## History — the only write path
-- Model mutations are commands, and the journal records them in the same
-- transaction as the rows they change (R-205). Undo does not rewind or delete
-- entries; it appends a marker naming what it reversed. History therefore never
-- lies, and the undo stack survives a restart for free.

-- Append-only command history. Rows are never updated or deleted.
CREATE TABLE journal (
  seq            INTEGER PRIMARY KEY,  -- order of application
  gesture        INTEGER NOT NULL,     -- the undo unit; one gesture may hold several commands
  kind           TEXT NOT NULL DEFAULT 'command' CHECK (kind IN ('command','undo','redo')),
                                       -- a command entry, or a marker recording that a gesture was reversed
                                       --   or reapplied
  target_gesture INTEGER,              -- which gesture a marker refers to; NULL on command entries
  ts             INTEGER NOT NULL,     -- epoch milliseconds
  command        TEXT,                 -- the command's name, denormalized so history is filterable in plain
                                       --   SQL ("every set_tempo ever")
  redo           TEXT,                 -- JSON: the RESOLVED command, with executor-assigned ids and timestamps
                                       --   filled in, so replay reproduces exactly and never renumbers
  undo           TEXT                  -- JSON: the inverse commands, computed when the command ran because
                                       --   removed rows cannot be re-derived later
);
CREATE INDEX journal_gesture_i ON journal(gesture);

-- A compaction waypoint. Dormant: the retention policy is keep-everything, so
-- nothing writes here yet (core-01). Present because trimming, if it ever
-- happens, must leave a marker saying history before this point is gone.
CREATE TABLE snapshot (
  id          INTEGER PRIMARY KEY,
                                 -- waypoint identity; nothing references it yet
  journal_seq INTEGER NOT NULL,  -- history is folded up to here
  ts          INTEGER NOT NULL,  -- epoch milliseconds
  state       TEXT               -- reserved for the R-203 JSON at that point, for coarse time travel
);

-- ## Realization — the executable specification
-- The view is the model's specification (§6f-bis): any optimized replacement
-- must match it, and it remains the oracle after it stops being the
-- implementation. Structural laws are what make it well-defined — the cycle
-- prohibition is its termination guarantee.

-- The arrangement as it will actually sound: direct events on a track, unioned
-- with events reached through an instance, with placement, looping, windowing
-- and transposition applied.
--
-- The window gates ONSETS. An event starting outside [offset, offset + length)
-- is dormant material, and a note whose duration crosses the window edge rings
-- out rather than being cut at every loop seam.
CREATE VIEW v_realized AS
SELECT e.track_id            AS track_id,
       e.kind                AS kind,
       e.at_tick             AS at_tick,
       e.dur_tick            AS dur_tick,
       e.note_number         AS note_number,
       e.velocity            AS velocity,
       ph.tuning_id          AS tuning_id,
       NULL                  AS phrase_instance_id
FROM event e
JOIN track t   ON t.id = e.track_id
JOIN phrase ph ON ph.id = t.phrase_id
WHERE e.track_id IS NOT NULL
UNION ALL
SELECT i.track_id,
       e.kind,
       i.at_tick + (l.value - 1) * p.length_tick + (e.at_tick - i.offset_tick),
       e.dur_tick,
       e.note_number + i.transpose,
       e.velocity,
       p.tuning_id,
       i.id
FROM phrase_instance i
JOIN phrase p ON p.id = i.phrase_id
CROSS JOIN generate_series(1, i.loop_count) l  -- a correlated table-valued argument: an
                                               -- implicit LATERAL, which SQLite permits.
                                               -- generate_series is a per-connection module,
                                               -- registered in project::configure
JOIN event e ON e.phrase_id = i.phrase_id
WHERE i.track_id IS NOT NULL
  AND i.mute = 0
  AND e.at_tick >= i.offset_tick
  AND e.at_tick - i.offset_tick < p.length_tick
  AND (l.value - 1) * p.length_tick + (e.at_tick - i.offset_tick)
      < COALESCE(i.length_tick, i.loop_count * p.length_tick);
