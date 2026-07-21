# misc-02 proposal — generated schema browser (and the file-map checker)

**Status: approved and implemented 2026-07-20** (all six decisions as
recommended). Minor item.

Implementation notes worth keeping: the annotation extractor identifies a column
by its first token being a lowercase identifier — which the naming standard
guarantees — so constraint and continuation lines stay out without a SQL parser,
and a `REFERENCES …` continuation still contributes its trailing comment to the
column above it. The file-map checker treats an entry as covering its ancestors
(`.github/workflows/` accounts for `.github`), and `meta.ignore` names what is
deliberately uncovered (build output, VCS, licence boilerplate) so the gap is
auditable rather than silent. Enforcement worked immediately: the first run
rejected ten unannotated columns. Two `xtask` commands that
emit committed JSON rendered by an HTML viewer on Pages, both CI-enforced current
— the pattern the plan dashboard and file map already established. No new
dependency: rusqlite and serde_json are in.

## 1. The DDL moves to a real SQL file

`src/rust/store/src/schema.sql`, pulled in with `include_str!`, so
`schema::DDL` is unchanged as far as the code is concerned. What changes is
everything around it: syntax highlighting in the editor and on GitHub, SQL-shaped
diffs, and `sqlite3 scratch.db < schema.sql` for poking at the schema without
building anything — which is the plain-files ethos applied to our own store.

**Annotation convention** (plain SQL comments; SQLite has no `COMMENT ON`, so
this is structurally the only place a note can sit beside its column):

```sql
-- ## Tuning — definition, binder, frozen result
-- The three-level stack: rules and exact ratios upstream, playback truth
-- downstream. (R-501..R-506)

-- A tuning definition: everything that determines the mapping.
CREATE TABLE tuning (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,  -- never reused; redo reinserts at this id
  anchor_note INTEGER NOT NULL,                   -- R-503; builtins anchor 60 = middle C
  ...
);
```

- `-- ##` opens an **architecture group** (name, then optional prose) and runs
  until the next one. Groups are the design's own layers, which is how the
  schema was built and how it should be read.
- Comment lines immediately above `CREATE TABLE`/`CREATE VIEW` are the **object
  note**.
- A trailing comment on a column line is its **column note**, continued by any
  comment-only lines that follow.

Annotating every column is part of this item; most are currently bare.

## 2. `cargo xtask schema` — structure from introspection, prose from comments

Structure is **read from a real database**, never parsed: the command creates a
throwaway project and interrogates `PRAGMA table_info`, `foreign_key_list`,
`index_list`/`index_info`, and `sqlite_master`. That yields columns, types,
nullability, defaults, primary keys, foreign keys, indexes and view SQL
authoritatively — it cannot drift from what the code actually creates, and it
reports things a parser would miss. Prose is extracted from `schema.sql` by the
convention above and matched onto that structure.

Output `doc/revision_schema.json` (generated, committed, never hand-edited),
carrying a `generated: true` marker and the group ordering.

**Pointed anywhere.** The command takes an optional path: with none it documents
a fresh project, with one it documents *that* project file. The introspection is
identical, so a user's own `.revision` file is browsable by the same tool — a
product feature in miniature for the cost of an argument.

## 3. `doc/revision_schema.html` — the viewer

- Tables grouped by architecture, in the SQL file's order, each with its note.
- Per column: name, type, constraints, default, and prose.
- **Foreign keys are links.** This schema is unusually relational and one table
  points at `phrase` twice with different roles (`phrase_id` the material,
  `parent_phrase_id` the container); navigating that by clicking beats reading it.
- **A mermaid ER diagram per group**, generated from the foreign-key list —
  which renders natively, so it costs a code fence rather than a dependency.
- `v_realized` shown with its SQL, since the view is the model's executable
  specification and deserves to be read as one.
- R-numbers surfaced from the notes.

## 4. `cargo xtask filemap` — the deferred checker

Implemented in the same pass, since it is the same shape. Two-way, per boot-03:
every entry's path exists, and every file or directory of consequence is covered
by an entry. Mismatches are failures, never warnings — the file map's usefulness
to agents rests entirely on being trustworthy.

## 5. CI

Both join the `lint` job: regenerate and fail if the committed JSON differs
(`xtask schema --check`), and fail on any file-map mismatch. Generated artifacts
that CI proves current are the only kind worth committing.

## 6. Decisions requested

1. DDL moves to `schema.sql` via `include_str!` — recommended: yes.
2. Annotation convention as in §1 (`-- ##` groups, object notes above, column
   notes trailing) — recommended: yes.
3. Structure by introspection of a real database rather than by parsing SQL —
   recommended: yes.
4. **Every column must carry a note, enforced by CI.** Strong forcing function
   for a self-documenting schema; the alternative is warn-only —
   recommended: yes, enforce.
5. `xtask schema [path]` doubles as a browser for any project file —
   recommended: yes.
6. `xtask filemap` implemented in the same pass, both wired into CI's lint job —
   recommended: yes.
