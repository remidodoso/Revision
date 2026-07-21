# Revision — agent orientation

A note-centric, degree-native compositional tool (Rust workspace). The
authoritative "how to work here" document is `doc/revision_getstarted.md` —
read it before doing anything substantive; it routes to everything else.

## File discovery

Consult `doc/revision_file_map.json` **before** enumerating directories or
opening files to learn where things live or where new code belongs. It is
small, retrieval-oriented, and CI-enforced current (an inconsistent map cannot
merge). Content search — symbols, callers, strings — is still ripgrep's job;
the map is for orientation and placement, not text search.

## Ground rules (never violate; details in getstarted)

- **Discussion precedes implementation.** Do not implement without the user's
  phrase "make it so."
- **Checkpoints** (schemas, subsystem names, layout, dependencies, public APIs,
  file formats) require a written proposal and explicit approval first. Live
  proposals sit in `doc/`; when their plan item completes, they move to
  `doc/completed/` (never deleted — they record why).
- **Commits are the user's, alone, always.** Never commit.
- **`../Notorolla` is strictly read-only.**
- **Interaction behaviour:** where the requirements are silent, the Macintosh
  Human Interface Guidelines (1992) decide it — cite the chapter, don't recall it
  (`doc/revision_hig_inventory.md`, R-939/R-940).
- **Coding conventions:** `doc/revision_coding_standard.md` — default is
  ordinary Rust convention; the doc records only deviations (singular names,
  sibling test modules, file-map bookkeeping, GPL-free dependencies, …).
- **Report honestly:** failing tests reported as failing; skipped steps
  reported as skipped.
