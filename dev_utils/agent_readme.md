# dev_utils — developer & agent utilities

Small, standalone tools for **inspecting and driving Revision during
development**. They are *not* part of the product and *not* part of the Rust
build: nothing here ships, nothing here is a workspace crate, and CI does not run
them. They exist so that recurring "let me just check…" tasks are one command
instead of an ad-hoc query typed from memory.

**Future agent: read this before hand-rolling a throwaway script.** If you find
yourself about to write a one-off to poke at the log, the store, or a device,
check whether one of these already does it — and if you write a new one, **add it
to the table below** so the next agent finds it instead of reinventing it.

## Conventions for tools that live here

- **Standard library only.** No `pip install`. A tool that needs a dependency
  does not belong in a repo whose whole posture is dependency-frugal — make it an
  `xtask` subcommand instead (that is the Rust-side automation home).
- **Read-only by default.** Anything that opens the observation log or a project
  store opens it read-only, so a running app is never disturbed. A tool that
  mutates state says so loudly and is opt-in.
- **Cross-platform path resolution mirrors the Rust.** The log path logic in
  `dump_log.py` mirrors `src/rust/log/src/place.rs`; if that file's rules change,
  change them here too.
- **Self-documenting.** Every tool has a `--help` and a docstring with examples.

## The tools

| Tool | What it does | Typical use |
|------|--------------|-------------|
| `dump_log.py` | Dumps a slice of the observation log (`observation.revlog`), the SQLite file every run appends to. Latest session by default; filter by session, creator, level, or text; list sessions. Relative timestamps like the app's stderr echo. | `python dev_utils/dump_log.py` — "what did the last run do?" After running a demo binary (`rev-rec`, `rev-mhall`, …), this is how you read what it observed without eyeballing scrollback. |

### `dump_log.py` quick reference

```
python dev_utils/dump_log.py                 # latest session, last 40 entries
python dev_utils/dump_log.py -n 100          # more history
python dev_utils/dump_log.py --sessions      # list runs, newest first
python dev_utils/dump_log.py --session 12    # one specific run
python dev_utils/dump_log.py --creator engine.transport
python dev_utils/dump_log.py --level warn    # warnings and errors only
python dev_utils/dump_log.py --grep note     # text contains "note"
python dev_utils/dump_log.py --path X.revlog # a log elsewhere
```

The log lives at `%LOCALAPPDATA%\Revision\observation.revlog` on Windows
(`~/Library/Application Support/Revision/` on macOS, `$XDG_DATA_HOME/revision/`
or `~/.local/share/revision/` on Linux). It is one forever file shared by every
run — a *session* is a column, not a filename (eng-01 §9.4) — which is why
cross-run questions ("has this xrun happened before?") are answerable at all.
