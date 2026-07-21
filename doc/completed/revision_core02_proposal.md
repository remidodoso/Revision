# core-02 proposal — model types + store (open/create/journal/replay)

**Status: approved and implemented 2026-07-20** (decision 7 amended by discussion —
localized query catalog; see §4). Implemented the core-01 design. Checkpoint content
per getstarted rule 2: public API shapes between crates, code organization, one new
dependency.

**Deviations from this proposal, and why** (recorded rather than silently followed):

- Timestamps are stored as integer epoch milliseconds, not ISO-8601 text. Formatting
  dates needs either calendar arithmetic or a dependency, and integers sort and
  compare exactly; rendering is a presentation concern.
- Model tables use `INTEGER PRIMARY KEY AUTOINCREMENT`. Plain row ids are *reused*
  after a delete, so an undo could free an id, a later create could claim it, and
  redoing the undone gesture — which reinserts at its original explicit id — would
  collide. Found by the property test, shrunk to `[Undo, CreatePhrase, Redo]`.
- Patches use a three-state `Change<T>` (`Leave` / `Clear` / `Set`) for nullable
  fields instead of `Option<T>`. An inverse must be able to restore a field to NULL,
  which `Option` cannot express; `Option<Option<T>>` does not survive a JSON round
  trip. Also found by the property test.
- `MaterializeTuning` carries a `ts` field under the same id discipline (`None` on
  issue, `Some` on replay): anything the executor supplies must reach the resolved
  payload, or history does not reproduce.
- Five `Remove*` commands were added beyond the core-01 vocabulary, because inverses
  are closed under the vocabulary and every create needs one (R-412 wants them
  regardless).
- CI's darwin cross-check moved to a macOS runner: bundled SQLite compiles C, and no
  Apple C toolchain exists on a Linux runner. Anticipated in boot-02 §5.

## 1. Scope

**In:** `rev-core` model types, tuning materialization (including the owned
equal-temperament math), command vocabulary; `rev-store` schema creation, project
open/create, genesis gesture, command execution, journal, undo/redo, replay;
`rev-testkit` fixtures (MHALL) and the first proptest strategies.

**Out (later items, deliberately):** the `v_realized` view is *created* by the schema
here but exercised and fixture-tested in core-03; JSON round-trip and the TMON kill
test are core-04; snapshot compaction is unimplemented (the table exists, dormant —
core-01 ruled keep-everything-forever); no editors, no engine, no UI.

## 2. Crate boundary

`rev-core` is pure: model types, pitch/tick math, command definitions, validation
that needs no database. **No rusqlite in `rev-core`** — CI's wasm32 check on that
crate enforces it (R-104). `rev-store` owns SQLite, execution, and any validation
requiring a query (cycle detection walks ancestry through the DB).

Dependencies: `rev-core` gains serde (derive); `rev-store` gains rusqlite (features
`bundled`, `series`, `hooks`, `backup`) and serde_json — all approved at core-01 §7.8.
**New request: `thiserror`** for typed library errors in both crates (§8.2).

## 3. Module layout

```
src/rust/core/src/
  lib.rs        re-exports; crate docs
  id.rs         PhraseId, EventId, TrackId, TuningId, ScaleId, …  (newtypes over i64)
  tick.rs       Tick, PPQ = 5040, tempo conversion (usec_per_quarter → seconds)
  note.rs       NoteNumber (signed), pitch_class via rem_euclid (the Arithmetic law)
  tuning/mod.rs Tuning, TuningKind, TuningNote, MaterializedTuning, materialize()
  tuning/equal.rs   owned exp2/pow for period^(k/N) — R-501 cross-platform identity
  scale.rs      Scale, mask membership / nearest / step (the Notorolla contract)
  phrase.rs     Phrase, Event, EventKind, PhraseInstance, Track, TempoPoint
  command.rs    Command enum + *Spec structs (serde; the journal payload format)
  error.rs      CoreError

src/rust/store/src/
  lib.rs        re-exports; crate docs
  schema.rs     DDL text, schema_version, create/verify
  project.rs    Project::create / ::open, connection setup (WAL, foreign_keys)
  gesture.rs    Gesture scope (transaction = gesture)
  exec.rs       Command → rows (id assignment, validation, per-command inverse)
  journal.rs    append, read, undo/redo cursor
  replay.rs     replay-from-empty; state comparison helper for tests
  genesis.rs    builtin tunings + scales as journaled commands
  error.rs      StoreError
```

Sibling `test.rs` modules per the coding standard; integration tests in
`tests/it/main.rs` per crate.

## 4. Public API shapes (the checkpoint content)

```rust
// ── rev-core ───────────────────────────────────────────────────────────────
pub const PPQ: i64 = 5040;
pub struct Tick(pub i64);
pub struct NoteNumber(pub i32);
impl NoteNumber { pub fn pitch_class(self, note_per_period: i32) -> i32; }  // rem_euclid

pub struct MaterializedTuning { /* first_note + Vec<f64> */ }
impl MaterializedTuning {
    pub fn freq(&self, n: NoteNumber) -> Option<f64>;        // O(1); None = out of domain
    pub fn nearest_note(&self, hz: f64) -> Option<NoteNumber>;   // binary search (monotone)
    pub fn note_range(&self) -> (NoteNumber, NoteNumber);
    pub fn note_per_period(&self) -> Option<i32>;            // None = aperiodic
    pub fn has_period(&self) -> bool;                        // the feature gate
}
pub fn materialize(t: &Tuning, note: &[TuningNote]) -> Result<MaterializedTuning, CoreError>;

pub enum Command {                    // serde-serialized = the journal payload
    CreateTuning { id: Option<TuningId>, tuning: TuningSpec },
    SetTuningNote { tuning_id: TuningId, note: Vec<TuningNoteSpec> },
    MaterializeTuning { tuning_id: TuningId, instance_id: Option<MaterializedTuningInstanceId> },
    CreateScale { id: Option<ScaleId>, scale: ScaleSpec },
    CreatePhrase { id: Option<PhraseId>, phrase: PhraseSpec },
    SetPhrase { id: PhraseId, patch: PhrasePatch },
    CreateTrack { id: Option<TrackId>, track: TrackSpec },
    AddEvent { container: Container, event: Vec<EventSpec> },   // EventSpec.id: Option<EventId>
    RemoveEvent { id: Vec<EventId> },
    CreatePhraseInstance { id: Option<PhraseInstanceId>, phrase_instance: PhraseInstanceSpec },
    SetPhraseInstanceParam { id: PhraseInstanceId, patch: PhraseInstancePatch },
    SetTempo { phrase_id: PhraseId, point: Vec<TempoPoint> },
    RecordBatch { track_id: TrackId, event: Vec<EventSpec> },
}

// ── rev-store ──────────────────────────────────────────────────────────────
pub struct Project { /* Connection */ }
impl Project {
    pub fn create(path: &Path) -> Result<Project, StoreError>;   // schema + genesis
    pub fn open(path: &Path) -> Result<Project, StoreError>;
    pub fn gesture<T>(&mut self, f: impl FnOnce(&mut Gesture) -> Result<T, StoreError>)
        -> Result<T, StoreError>;                                // commit on Ok, rollback on Err
    pub fn undo(&mut self) -> Result<bool, StoreError>;          // false = nothing to undo
    pub fn redo(&mut self) -> Result<bool, StoreError>;
    pub fn connection(&self) -> &Connection;                     // reads are free (dogfooding)
}
pub struct Gesture<'a> { /* &mut Transaction */ }
impl Gesture<'_> { pub fn exec(&mut self, cmd: Command) -> Result<Command, StoreError>; }
                                        // returns the RESOLVED command (ids filled in)
```

**Id assignment, uniformly:** every creating command carries `id: Option<…>`. `None`
means "executor allocates"; `Some` means "use exactly this id" — which is what replay
and redo pass, so reproduction is exact and no renumbering can occur. `exec` returns
the resolved command; the journal stores that.

**Reads go through a localized query catalog** (amended by discussion 2026-07-20).
What is rejected is a *semantic* layer that hides the schema and becomes the
privileged path (the §6g disease). What is built is a catalog: all SQL localized to
`rev-store::query`, so column names appear in exactly two places — the DDL and the
catalog — and every other crate speaks in `rev-core` types (rusqlite stops leaking
into consumers). Four rules keep a catalog from becoming a layer:

1. One function, one statement — no orchestration, branching, or caching. Read logic
   that recurs becomes a `v_*` view (shared surface), never a private Rust helper.
2. Read-only by construction — no mutating functions; writes are commands.
3. Named for what it returns, not for the caller.
4. Every catalog function has a test — SQL is runtime-checked, so a missed rename must
   fail a test rather than ship.

Capability parity holds through the scripting surface (§6f: raw read-only SQL over
one's own project), since every catalog entry is one statement a script could write.
`Project::reader()` hands back a `SQLITE_OPEN_READ_ONLY` connection, so "commands are
the only writer" is enforced by SQLite rather than by convention; it also cannot see
an in-flight gesture's uncommitted rows, which is correct (the database is
authoritative at commit boundaries).

**Performance escape hatches are sanctioned** (§6f-bis): when a view groans, a
temp-table waypoint or Rust post-processing may replace it *inside* a catalog
function, with no caller change — and the view remains the oracle it is property-
tested against. Materializations are caches: derived, never truth, never journaled,
never in the interchange format. The trigger is measurement (the perf ledger), not
anticipation.

## 5. Genesis content

12-ET and 16-ET (`equal`), 5-limit JI (`table`, exact ratios), each materialized over
the piano-plus range; scale masks ported from Notorolla's library (read-only
derivation of the user's own data — 12-ET modes, symmetric scales, blues,
pentatonics; 16-ET Mavila family, octatonic, whole-tone, Lemba). Chromatic is not a
row (NULL binding). All issued as ordinary journaled commands in gesture 1.

## 6. Testing

- **Unit:** tick/tempo conversion; `pitch_class` across negative note numbers;
  materialization for all three kinds; equal-temperament golden values (12-ET
  A4 = 440, middle C, octave doubling exact; 16-ET step = 75¢); `nearest_note`
  monotonicity; scale membership/nearest/step against Notorolla-derived expectations.
- **Integration:** create → execute → verify rows; undo/redo round-trip; genesis
  content present after `create`; reopen sees committed state.
- **Property (proptest, first consumer):** for any valid command sequence,
  (a) replay-from-empty reproduces byte-identical table state, and (b) execute-then-
  undo restores the prior state exactly. Regressions committed per boot-03.
- **Fixture (rev-testkit):** `mhall()` builds the Mary-Had-a-Little-Lamb phrase used
  by core-03, eng-06/07 and the 16-ET party trick.

No `unsafe` anywhere in core-02; a policy discussion is not needed yet.

## 7. Exit criteria

`Project::create` produces a schema-complete, genesis-populated store; commands
execute in gestures with journal rows written in the same transaction; undo/redo
work across a reopen; replay-from-empty reproduces state; MHALL exists as a fixture;
all tests green; `cargo fmt`/`clippy -D warnings`/wasm32 check on `rev-core` clean.

## 8. Decisions requested

1. Module layout and public API shapes as in §3–§4 — recommended: yes.
2. **New dependency `thiserror`** (MIT/Apache, derive-only, no runtime) for typed
   errors in both library crates — recommended: yes.
3. Id newtypes over bare `i64` — recommended: yes (cheap, prevents mixing ids).
4. Uniform `id: Option<…>` on creating commands (None = allocate, Some = exact) —
   recommended: yes.
5. `gesture(|g| …)` closure scope rather than RAII begin/commit — recommended: yes
   (rollback on error or panic is automatic; misuse is hard).
6. Genesis seeds the full Notorolla-derived scale library rather than a minimal set —
   recommended: yes (cheap, and the 16-ET party trick wants Mavila present).
7. Reads via a localized read-only query catalog (`rev-store::query`) over a
   `reader()` connection, views as the reuse mechanism, sanctioned performance escape
   hatches — **amended and ruled yes** (2026-07-20).
8. Scope split confirmed: view created here but tested in core-03; JSON round-trip and
   TMON in core-04; snapshot dormant — recommended: yes.
