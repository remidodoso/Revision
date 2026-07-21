# core-01 proposal — schema DDL, command vocabulary, journal format

**Status: approved and implemented; core-01 closed 2026-07-19** (the requirements
edit list noted below was applied the same day). Implemented by core-02. Retained as
the record of what was decided and why.

*Original header:* **decisions 1–8 ruled by user 2026-07-19** (4 amended to 16-bit
velocity); **phrase pass ruled and folded 2026-07-19** (scale binding on
phrase; window semantics for length + `offset_tick`; make/unmake as composite
gestures; R-414 attributes stay in `extra`; R-415 v0 fallbacks;
drop-out-of-domain). Outstanding before core-01 closes: the user-owned
requirements edit list (terminology + R-509). Concretizes the revision_poc "Core schema sketch v0"
against requirements §3/§5/§6; the realization view is designed alongside it
(§6f-bis: the view is the executable specification).

## 1. Ground decisions visible throughout

- **Names:** singular tables/columns, snake_case. Tick-valued columns end `_tick`
  (`at_tick`, `dur_tick`, `length_tick`) — unit explicit, singular per standard.
  PKs are `id`; FKs are `<table>_id`, role-qualified when duplicated or
  self-referential (`parent_phrase_id`). Overloaded concept words are always
  qualified — no bare `instance` anywhere (`phrase_instance`,
  `materialized_tuning_instance`). Long FK names are the price; we'll survive.
- **Time:** integer ticks, 5040 ppq (R-003). Tempo as **integer microseconds per
  quarter** (MIDI-exact; no float drift; division happens in core, deterministically).
- **Dual representation, one writer:** current-state tables hold the model (not
  pure event sourcing); the journal holds history. Both written in the **same
  transaction** — gesture = transaction. Commands are the only writer (dogfooding
  rule); reads are free.
- **JSON `extra` escape valves** on every content table: R-402/R-405/R-414
  extensibility without premature columns; a field graduates to a column when it
  earns an index or a CHECK.

## 2. DDL

```sql
PRAGMA journal_mode = WAL;            -- at create; foreign_keys=ON per connection
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- keys: schema_version, ppq='5040', name, created, root_phrase_id (R-408:
-- any phrase may be root; current root is a setting), default_tuning_id,
-- journal_cursor (undo position)

-- ── Tuning stack: definition → binder → materialized result ──────────────────
-- (Design developed 2026-07-19 from the Notorolla consumer inventory; the
-- materialized rows are playback truth — frozen floats, full domain — while the
-- definition side stays exact. Note numbers are SIGNED; anchor-at-60 is a
-- builtin convention, not schema.)

CREATE TABLE tuning (                 -- the DEFINITION: everything determining the mapping
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  description TEXT,                   -- NULL on derived rows -> display parent's (no drift)
  kind TEXT NOT NULL CHECK (kind IN ('equal','table')),
  period_num INTEGER,                 -- exact period ratio (2/1, 3/1 BP);
  period_den INTEGER,                 --   both NULL = aperiodic (R-502)
  note_per_period INTEGER,            -- the modulus; NULL = aperiodic
  anchor_note INTEGER NOT NULL,       -- R-503; builtins anchor 60 = middle C
  anchor_freq REAL NOT NULL,          -- cross-tuning continuity = same anchor freq
  note_min INTEGER,                   -- declared domain for unbounded kinds;
  note_max INTEGER,                   --   NULL for aperiodic table (domain = its rows)
  naming TEXT,                        -- R-508 scheme: letter | hex | near12 | NULL = numeric
  origin TEXT,                        -- provenance (R-413): builtin|generated|derived|user
  seed TEXT,                          -- recipe: generator name/params/seed (JSON)
  parent_tuning_id INTEGER REFERENCES tuning(id),  -- derived/re-rooted lineage (R-517 open)
  extra TEXT NOT NULL DEFAULT '{}',
  CHECK ((period_num IS NULL) = (period_den IS NULL)),
  CHECK ((period_num IS NULL) = (note_per_period IS NULL))
);

CREATE TABLE tuning_note (            -- definitional rows for kind='table' (exact; R-504)
  tuning_id INTEGER NOT NULL REFERENCES tuning(id),
  note_number INTEGER NOT NULL,       -- absolute; periodic: one canonical period
  ratio_num INTEGER,                  --   [anchor_note, anchor_note + N)
  ratio_den INTEGER,                  -- exact ratio from anchor_freq ...
  freq REAL,                          -- ... XOR direct frequency (measured/hand-assigned)
  PRIMARY KEY (tuning_id, note_number),
  CHECK ((ratio_num IS NULL) = (ratio_den IS NULL)),
  CHECK ((ratio_num IS NOT NULL) <> (freq IS NOT NULL))
) WITHOUT ROWID;

CREATE TABLE materialized_tuning_instance (  -- pure binder: names one materialization
  id INTEGER PRIMARY KEY,
  tuning_id INTEGER NOT NULL REFERENCES tuning(id),
  ts TEXT NOT NULL
);

CREATE TABLE materialized_tuning (    -- the RESULT: frozen, full-domain, playback truth
  materialized_tuning_instance_id INTEGER NOT NULL
    REFERENCES materialized_tuning_instance(id),
  note_number INTEGER NOT NULL,
  freq REAL NOT NULL,
  PRIMARY KEY (materialized_tuning_instance_id, note_number)
) WITHOUT ROWID;

CREATE TABLE scale (                  -- a SHAPE; root is always a use-site parameter
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  note_per_period INTEGER,            -- structural parent = the modulus (periodic scales;
                                      --   one mask serves 12-ET AND 5-limit JI)
  tuning_id INTEGER REFERENCES tuning(id),  -- aperiodic scales only (R-509's other branch)
  mask TEXT NOT NULL,                 -- JSON: periodic = root-relative offsets in
                                      --   [0, note_per_period); aperiodic = absolute notes
  origin TEXT,
  seed TEXT,
  parent_scale_id INTEGER REFERENCES scale(id),   -- e.g. modes: dorian -> major
  extra TEXT NOT NULL DEFAULT '{}',
  CHECK ((note_per_period IS NULL) <> (tuning_id IS NULL))
);
-- No chromatic row: a NULL scale binding IS chromatic (R-510). Scale<->tuning
-- idiomatic fit ("makes sense over") is advisory/computed, never schema-enforced.
-- Scales get NO persisted materialization: exact integer recompute, no drift risk
-- (the asymmetry with tunings is deliberate - their materialization is float
-- results from a version-dependent builder).

CREATE TABLE phrase (                 -- R-401; the unit of material
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  length_tick INTEGER NOT NULL CHECK (length_tick >= 0),   -- explicit (R-401): the WINDOW —
                                      --   gates onsets, sets loop stride; events beyond it
                                      --   are dormant material (window model, ruled 2026-07-19)
  tuning_id INTEGER REFERENCES tuning(id),                 -- nullable (R-414/R-418)
  scale_id INTEGER REFERENCES scale(id),                   -- NULL = chromatic (R-510)
  root INTEGER NOT NULL DEFAULT 0,    -- pitch class rooting the mask (R-517 open;
                                      --   pinned 0 for aperiodic tunings)
  origin TEXT,                        -- R-413: recorded|generated|derived|NULL
  seed TEXT,                          -- R-413: seed + generator version/params (JSON)
  parent_phrase_id INTEGER REFERENCES phrase(id),          -- R-413 parent material
  extra TEXT NOT NULL DEFAULT '{}'    -- meter map, instrument binding (R-414) until earned
);

CREATE TABLE track (                  -- R-406/R-425: one kind of track
  id INTEGER PRIMARY KEY,
  phrase_id INTEGER NOT NULL REFERENCES phrase(id),  -- owning root phrase (PoC narrowing)
  name TEXT NOT NULL,
  ord INTEGER NOT NULL,               -- sibling order
  extra TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE event (                  -- R-402; container: phrase XOR track
  id INTEGER PRIMARY KEY,
  phrase_id INTEGER REFERENCES phrase(id),
  track_id INTEGER REFERENCES track(id),             -- direct events (R-406)
  kind TEXT NOT NULL DEFAULT 'note',  -- extensible: cc, articulation, audio (R-402/R-425)
  at_tick INTEGER NOT NULL CHECK (at_tick >= 0),
  dur_tick INTEGER NOT NULL DEFAULT 0 CHECK (dur_tick >= 0),
  note_number INTEGER,                -- signed; NULL for unpitched kinds
  velocity INTEGER,                   -- 0..65535 (MIDI 2.0 16-bit; 7-bit translated
                                      --   at the MIDI boundary per spec; UI shows 0..127)
  extra TEXT NOT NULL DEFAULT '{}',
  CHECK ((phrase_id IS NULL) <> (track_id IS NULL))
);
CREATE INDEX event_phrase_id_at_tick_i ON event(phrase_id, at_tick) WHERE phrase_id IS NOT NULL;
CREATE INDEX event_track_id_at_tick_i  ON event(track_id,  at_tick) WHERE track_id  IS NOT NULL;

CREATE TABLE phrase_instance (        -- R-404/405/407; container: track XOR parent phrase
  id INTEGER PRIMARY KEY,
  phrase_id INTEGER NOT NULL REFERENCES phrase(id),  -- the referenced material
  track_id INTEGER REFERENCES track(id),
  parent_phrase_id INTEGER REFERENCES phrase(id),    -- nesting (R-407)
  at_tick INTEGER NOT NULL CHECK (at_tick >= 0),
  offset_tick INTEGER NOT NULL DEFAULT 0 CHECK (offset_tick >= 0),
                                      -- window START into the material (play bars 8-16
                                      --   of 32); completes the Vision §3a parameter set
  length_tick INTEGER,                -- NULL = natural extent (R-405 independent length)
  loop_count INTEGER NOT NULL DEFAULT 1 CHECK (loop_count >= 1),
  transpose INTEGER NOT NULL DEFAULT 0,   -- note numbers, in material's tuning (R-423)
  mute INTEGER NOT NULL DEFAULT 0 CHECK (mute IN (0,1)),
  extra TEXT NOT NULL DEFAULT '{}',   -- articulation template, timbre arc (R-405 extensible)
  CHECK ((track_id IS NULL) <> (parent_phrase_id IS NULL))
);
CREATE INDEX phrase_instance_track_id_at_tick_i
  ON phrase_instance(track_id, at_tick) WHERE track_id IS NOT NULL;
CREATE INDEX phrase_instance_phrase_id_i
  ON phrase_instance(phrase_id);      -- where-used (R-411)

CREATE TABLE tempo_point (            -- R-003/R-416; owner phrase (R-414)
  id INTEGER PRIMARY KEY,
  phrase_id INTEGER NOT NULL REFERENCES phrase(id),
  at_tick INTEGER NOT NULL CHECK (at_tick >= 0),
  usec_per_quarter INTEGER NOT NULL CHECK (usec_per_quarter > 0)
);

CREATE TABLE journal (                -- R-205; append-only, rows immutable
  seq INTEGER PRIMARY KEY,
  gesture INTEGER NOT NULL,           -- undo unit; gesture = transaction
  kind TEXT NOT NULL DEFAULT 'command' CHECK (kind IN ('command','undo','redo')),
  target_gesture INTEGER,             -- for undo/redo entries
  ts TEXT NOT NULL,                   -- ISO 8601 UTC
  command TEXT,                       -- command name (NULL on undo/redo entries)
  redo TEXT,                          -- JSON: resolved command (ids filled in)
  undo TEXT                           -- JSON: inverse command(s)
);

CREATE TABLE snapshot (               -- compaction waypoint
  id INTEGER PRIMARY KEY,
  journal_seq INTEGER NOT NULL,       -- journal folded up to here
  ts TEXT NOT NULL,
  state TEXT                          -- nullable; reserved for time-travel (R-203 JSON)
);
```

Not expressible in SQL, enforced in core (with tests): **cycle prohibition**
(R-407 — ancestry walk on CreatePhraseInstance/reparent), per-kind event field
validity, R-405 length semantics; tuning-side: periodic `tuning_note` coverage
(exactly one canonical period, contiguous, containing the anchor), aperiodic
domain checks (out-of-domain note numbers rejected at command level), scale
mask validity against its modulus. R-tree indices deferred until editors.

**Pitch-math laws (implementation notes for core-02):** all pitch-class
arithmetic uses euclidean modulo (`rem_euclid`, never bare `%` — note numbers
are signed); the `equal` kind's `period^(k/N)` uses core-owned exponentiation
(libm `pow` is not bit-identical cross-platform; R-501), golden-value tested,
covered by the native↔WASM conformance gate. Rational and table paths are
bit-exact by construction (IEEE division is correctly rounded).

**Terminology settled (requirements §6 Definitions aligned):** note number =
the datum (signed; `event.note_number`); pitch class = note number modulo
notes-per-period (periodic tunings only; derived, never stored); degree = a
position within a *scale* — the classically defined, user-facing term; midi
note = the 0–127 wire value (MIDI layer only). Transpose vocabulary: **by
degree** (scale-relative) vs **chromatically** (by note number — equivalently,
degree transposition under the null scale).

## 3. Realization view v0

Track-level instances only (nesting upgrade = recursive CTE later, per sketch).
Loop unrolling via `generate_series` (rusqlite `series` feature):

```sql
CREATE VIEW v_realized AS
SELECT e.track_id, e.kind, e.at_tick, e.dur_tick, e.note_number, e.velocity, e.extra,
       ph.tuning_id, NULL AS phrase_instance_id
FROM event e JOIN track t ON t.id = e.track_id
             JOIN phrase ph ON ph.id = t.phrase_id      -- direct events: root's tuning
WHERE e.track_id IS NOT NULL
UNION ALL
SELECT i.track_id, e.kind,
       i.at_tick + (l.value - 1) * p.length_tick
         + (e.at_tick - i.offset_tick) AS at_tick,
       e.dur_tick,
       e.note_number + i.transpose AS note_number,      -- transpose in material tuning (R-423)
       e.velocity, e.extra,
       p.tuning_id,                                     -- notes read in phrase tuning (R-418)
       i.id AS phrase_instance_id
FROM phrase_instance i
JOIN phrase p ON p.id = i.phrase_id
CROSS JOIN generate_series(1, i.loop_count) l  -- correlated TVF arg: an implicit
                                               --   LATERAL, which SQLite permits
JOIN event e ON e.phrase_id = i.phrase_id
WHERE i.track_id IS NOT NULL AND i.mute = 0
  AND e.at_tick >= i.offset_tick                       -- window start (offset into material)
  AND e.at_tick - i.offset_tick < p.length_tick        -- window gates ONSETS; tails ring
  AND (l.value - 1) * p.length_tick + (e.at_tick - i.offset_tick)
      < COALESCE(i.length_tick, i.loop_count * p.length_tick);   -- R-405 length clip
```

**Window semantics (ruled 2026-07-19):** a phrase is a window of playability
over potentially longer material. `length_tick` gates *onsets* (events starting
at/beyond it are dormant — present, preserved, silent) and sets the loop
stride; sounding notes whose duration crosses the window edge ring out
naturally (truncate-at-edge would staccato every loop seam; a truncation
option is a future per-instance parameter). `offset_tick` is the window's
start into the material. Length is fixed once set — defaulted from content
extent at creation, changed only by explicit `SetPhrase`, never silently by
edits: shrink = non-destructive mute of the tail, grow = reveal.

**R-415 v0 fallbacks (explicitly non-precedent while the requirement is
[Open]):** tuning — phrase's, else `meta.default_tuning_id`; tempo — the
arrangement root's `tempo_point` rows govern everything (nested phrases' own
tempo ignored until R-416 polytempo work); scale/root — phrase's own,
uninherited (editor/generator guidance only, R-510).

The stage-2 schedule compiler consumes `v_realized ORDER BY at_tick`, resolving
pitch through the governing tuning's **latest `materialized_tuning_instance`**
(latest-instance-wins is the v0 resolution rule; pinning a specific instance is
anticipated but not yet needed — the binder id is the dynamic-tuning funnel).
**Out-of-domain note numbers (e.g. after transpose) are dropped at compile,
with editors expected to warn** — a note that can't sound at its pitch doesn't
sound at another (ruled 2026-07-19; less musical to invent a weird note than
to miss an unrealizable one). Fixture tests:
hand-computed expected rows for looped/transposed/clipped/muted cases (core-03).

## 4. Command vocabulary v0

Rust enum in rev-core; serde-serialized JSON is the journal payload format and
seeds the R-203 interchange serializer. Names singular per standard (renames
from the sketch: AddEvents→AddEvent etc.); batch commands carry `Vec` fields
named in the singular (`event: Vec<EventSpec>`).

```
CreatePhrase   { phrase: PhraseSpec }
SetPhrase      { id, patch }              -- name/length/tuning; the 16-ET swap is this
AddEvent       { container, event: Vec<EventSpec> }
RemoveEvent    { id: Vec<EventId> }
CreatePhraseInstance { phrase_instance: PhraseInstanceSpec }
SetPhraseInstanceParam { id, patch }      -- offset/length/loop/transpose/mute (R-405)
CreateTrack    { track: TrackSpec }
SetTempo       { phrase_id, point: Vec<TempoPoint> }
CreateTuning   { tuning: TuningSpec }
SetTuningNote  { tuning_id, note: Vec<TuningNoteSpec> }  -- "this frequency is this note"
MaterializeTuning { tuning_id }           -- builder runs, binder + frozen rows land
CreateScale    { scale: ScaleSpec }
RecordBatch    { track_id, event: Vec<EventSpec> }   -- capture path (R-807 seed)
```

Make-phrase / unmake (R-409) are **composite gestures over these primitives**
(make = CreatePhrase + move events + CreatePhraseInstance; unmake = inverse) —
no dedicated commands in v0; one gesture = one undo; losslessness falls out of
resolved-id journaling. (Ruled 2026-07-19; revisit if practice disagrees.)

**Journal semantics:** executor assigns ids, then journals the **resolved** redo
payload (replay is exact, no id renumbering) and the computed inverse as the
undo payload (e.g. RemoveEvent's undo carries the removed rows). Undo/redo are
themselves appended as journal entries (`kind`, `target_gesture`) — the journal
never rewrites, the undo stack survives restarts (R-205), and `journal_cursor`
in meta tracks position. Compaction: fold into tables (already current), write
`snapshot` waypoint, trim entries older than the retention policy.

## 5. Project creation

New store: schema + the **genesis gesture** — because the journal is the only
write path, project creation is itself journaled: meta settings (`ppq=5040`,
versions), then builtins as ordinary commands — `CreateTuning` for 12-ET and
16-ET (`equal`) and 5-limit JI (`table`, exact ratios via `SetTuningNote`),
`MaterializeTuning` for each, `CreateScale` for the standard masks (the
Notorolla mask library imports nearly verbatim). Replay-from-empty reconstructs
everything; the R-203 JSON embeds tunings with zero special-casing (R-506);
user tunings arrive through the identical commands (first-class, R-505).

## 6. Dependencies entering with implementation (core-02)

This checkpoint covers them (getstarted rule 2): **rusqlite** (MIT; `bundled`
SQLite, `series` for generate_series, `hooks` for cache invalidation, `backup`)
and **serde + serde_json** (dual MIT/Apache) for command/journal/R-203 JSON.
serde_json also unblocks the flagged `xtask filemap` implementation. All on the
deny.toml allowlist already. proptest (approved, boot-03) gets its first
consumer here.

## 7. Decisions requested

1. DDL as specified (§2): table set, XOR containers, `_tick` columns, partial
   indices; cycle check in core — recommended: yes.
2. Tempo as integer `usec_per_quarter` points owned by a phrase — recommended: yes.
3. Tuning/scale table set as revised: definition (`tuning` + exact
   `tuning_note` rows) → pure binder (`materialized_tuning_instance`) → frozen
   result (`materialized_tuning`, persisted playback truth); modulus-keyed
   `scale` with the aperiodic-XOR-tuning branch; signed note numbers,
   anchor-60 convention; genesis gesture; core-owned equal-temperament math —
   recommended: yes.
4. Velocity as integer 0–65535 (MIDI 2.0 16-bit domain; spec-defined 7-bit
   translation at the MIDI boundary; UI presents 0–127 by default) — amended
   from 0–127 at user direction, ruled yes.
5. Journal design (§4): append-only with undo/redo as entries, resolved-id redo
   payloads, inverse-command undo payloads, gesture = transaction —
   recommended: yes.
6. Command vocabulary v0 (§4), singular names — recommended: yes.
7. Realization view v0 (§3) as the executable specification — recommended: yes.
8. Dependencies: rusqlite (+features) and serde/serde_json — recommended: yes.
