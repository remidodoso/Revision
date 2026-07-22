#!/usr/bin/env python3
"""Dump a slice of Revision's observation log.

The log is one forever-growing SQLite file in the OS application-data directory
(see src/rust/log/src/place.rs), shared by every run. This reads it — no
dependencies beyond the standard library — so "what just happened?" is one
command instead of a hand-written query.

Examples:
    python dev_utils/dump_log.py                 # latest session, last 40 entries
    python dev_utils/dump_log.py -n 100          # last 100 of the latest session
    python dev_utils/dump_log.py --sessions      # list runs, newest first
    python dev_utils/dump_log.py --session 12    # a specific run
    python dev_utils/dump_log.py --creator engine.transport
    python dev_utils/dump_log.py --level warn    # warnings and errors only
    python dev_utils/dump_log.py --grep "note"   # text contains "note"
    python dev_utils/dump_log.py --path some/other.revlog
"""

import argparse
import os
import sqlite3
import sys

# 0 trace, 1 info, 2 warn, 3 error — mirrors rev_log::Level (log/src/lib.rs).
LEVEL_NAME = {0: "TRACE", 1: "INFO", 2: "WARN", 3: "ERROR"}
LEVEL_NUM = {name.lower(): num for num, name in LEVEL_NAME.items()}


def default_log_path():
    """Where the log lives, per platform — the mirror of place.rs."""
    if sys.platform.startswith("win"):
        base = os.environ.get("LOCALAPPDATA")
        if not base:
            raise SystemExit("LOCALAPPDATA is unset; pass --path")
        return os.path.join(base, "Revision", "observation.revlog")
    if sys.platform == "darwin":
        home = os.path.expanduser("~")
        return os.path.join(home, "Library", "Application Support", "Revision",
                            "observation.revlog")
    xdg = os.environ.get("XDG_DATA_HOME")
    if xdg and os.path.isabs(xdg):
        return os.path.join(xdg, "revision", "observation.revlog")
    return os.path.join(os.path.expanduser("~"), ".local", "share", "revision",
                        "observation.revlog")


def list_sessions(conn):
    rows = conn.execute(
        "SELECT id, started, version, platform, build FROM session "
        "ORDER BY id DESC"
    ).fetchall()
    if not rows:
        print("no sessions in the log")
        return
    print(f"{'id':>5}  {'started (unix us)':>18}  version   build     platform")
    for sid, started, version, build, platform in (
        (r[0], r[1], r[2], r[4], r[3]) for r in rows
    ):
        print(f"{sid:>5}  {started:>18}  {version:<8}  {build:<8}  {platform}")


def latest_session(conn):
    row = conn.execute("SELECT MAX(id) FROM session").fetchone()
    return row[0] if row else None


def session_start(conn, session_id):
    row = conn.execute(
        "SELECT started FROM session WHERE id = ?", (session_id,)
    ).fetchone()
    return row[0] if row else None


def dump(conn, args):
    where = []
    params = []
    if args.session is not None:
        where.append("session_id = ?")
        params.append(args.session)
    if args.creator:
        where.append("creator LIKE ?")
        params.append(f"%{args.creator}%")
    if args.level:
        where.append("level >= ?")
        params.append(LEVEL_NUM[args.level])
    if args.grep:
        where.append("text LIKE ?")
        params.append(f"%{args.grep}%")
    clause = ("WHERE " + " AND ".join(where)) if where else ""

    # Newest N by id (the authoritative ordering), then shown oldest→newest so a
    # run reads top to bottom the way it happened.
    rows = conn.execute(
        f"SELECT id, session_id, ts, creator, level, text FROM entry "
        f"{clause} ORDER BY id DESC LIMIT ?",
        (*params, args.limit),
    ).fetchall()
    rows.reverse()
    if not rows:
        print("no matching entries")
        return

    # Relative time is measured from the session start, like the stderr echo's
    # "[    0.074]" — but only when a single session is in view.
    single = {r[1] for r in rows}
    base = session_start(conn, next(iter(single))) if len(single) == 1 else None

    for _id, session_id, ts, creator, level, text in rows:
        if base is not None:
            stamp = f"[{(ts - base) / 1e6:9.3f}]"
        else:
            stamp = f"s{session_id} {ts}"
        print(f"{stamp} {LEVEL_NAME.get(level, level):<5} {creator:<18} {text}")


def main():
    parser = argparse.ArgumentParser(
        description="Dump a slice of Revision's observation log.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("--path", help="log file (default: the OS app-data path)")
    parser.add_argument("-n", "--limit", type=int, default=40,
                        help="how many recent entries (default 40)")
    parser.add_argument("--session", type=int,
                        help="a specific session id (default: the latest)")
    parser.add_argument("--all-sessions", action="store_true",
                        help="do not restrict to one session")
    parser.add_argument("--sessions", action="store_true",
                        help="list sessions instead of entries")
    parser.add_argument("--creator", help="only entries whose creator contains this")
    parser.add_argument("--level", choices=sorted(LEVEL_NUM),
                        help="minimum level (trace|info|warn|error)")
    parser.add_argument("--grep", help="only entries whose text contains this")
    args = parser.parse_args()

    path = args.path or default_log_path()
    if not os.path.exists(path):
        raise SystemExit(f"no log at {path} (run the app once, or pass --path)")

    # Read-only, so a running app is never disturbed.
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        if args.sessions:
            list_sessions(conn)
            return
        if not args.all_sessions and args.session is None:
            args.session = latest_session(conn)
        dump(conn, args)
    finally:
        conn.close()


if __name__ == "__main__":
    main()
