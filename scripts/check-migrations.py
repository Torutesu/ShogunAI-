#!/usr/bin/env python3
"""Schema backward-compatibility guard (CLAUDE.md: memory lives for years; never break compat).

Two independent failure modes are checked, because each one alone can end a years-old memory.

1. CONTENT — migrations must be additive. A DROP COLUMN / DROP TABLE / RENAME / ALTER COLUMN
   breaks readers of older data. This scans the refinery migration SQL and fails on any
   backward-incompatible statement.

2. VERSION ORDER — migrations must be *appended*, never inserted below what already exists.
   This half was missing, and it cost a database: on 2026-08-09 `V16__lessons` was committed at
   08:34:10 and `V15__briefs` at 08:35:01, 51 seconds later with a lower number. refinery treats
   "a migration on the filesystem whose version is below the highest applied one and was never
   applied" as a hard error, so every store migrated in that window stopped opening — capture,
   search and drafting all dead. `shogun_memory::repair_skipped_migrations` heals the databases
   already in that state; this check is what stops another one being created.

Run from the repo root:  python3 scripts/check-migrations.py [base-ref]
Self-test the detectors:  python3 scripts/check-migrations.py --self-test

`base-ref` (default `origin/main`) is what the version-order check diffs against. When git or the
base ref is unavailable the order check reports itself as skipped rather than passing silently.
"""

import pathlib
import re
import subprocess
import sys

MIGRATIONS_DIR = "crates/shogun-memory/src/migrations"

# Backward-incompatible statements (case-insensitive). Additive DDL (CREATE, ADD COLUMN) is fine.
BANNED = [
    (re.compile(r"\bDROP\s+TABLE\b", re.IGNORECASE), "DROP TABLE"),
    (re.compile(r"\bDROP\s+COLUMN\b", re.IGNORECASE), "DROP COLUMN"),
    (re.compile(r"\bRENAME\s+COLUMN\b", re.IGNORECASE), "RENAME COLUMN"),
    (re.compile(r"\bRENAME\s+TO\b", re.IGNORECASE), "RENAME TABLE"),
    # `ALTER ... DROP` / `ALTER COLUMN ... TYPE` — a type/shape change breaks old readers.
    (re.compile(r"\bALTER\s+COLUMN\b", re.IGNORECASE), "ALTER COLUMN"),
]


def strip_line_comments(text):
    """Drop `-- ...` comments so a banned keyword in prose doesn't false-positive."""
    out = []
    for line in text.splitlines():
        idx = line.find("--")
        out.append(line[:idx] if idx >= 0 else line)
    return "\n".join(out)


# A migration may declare a sanctioned exception with a `-- non-additive-ok: <reason>` line,
# but ONLY if the file also carries a rollback plan (CLAUDE.md: unavoidable changes need a
# decision record + rollback plan). Data-preserving table rebuilds (SQLite cannot ALTER a CHECK
# in place: create-copy-drop-rename) are the intended use; the reason line IS the decision record.
EXCEPTION_MARK = re.compile(r"^--\s*non-additive-ok:\s*\S", re.IGNORECASE | re.MULTILINE)
ROLLBACK_MARK = re.compile(r"ロールバック手順|rollback", re.IGNORECASE)


def sanctioned(text):
    """True when the file declares a non-additive exception AND documents a rollback plan."""
    return bool(EXCEPTION_MARK.search(text)) and bool(ROLLBACK_MARK.search(text))


def scan(files):
    """Yield (path, label, snippet) for every banned statement in the migration SQL."""
    hits = []
    for path, text in files:
        if sanctioned(text):
            continue
        code = strip_line_comments(text)
        for regex, label in BANNED:
            for m in regex.finditer(code):
                start = max(0, m.start() - 20)
                hits.append((path, label, code[start : m.end() + 20].strip().replace("\n", " ")))
    return hits


VERSION_RE = re.compile(r"^V(\d+)__")


def numbered(names):
    """[(version, basename)] for every migration filename that carries one, in version order."""
    out = []
    for n in names:
        base = pathlib.PurePosixPath(n).name
        m = VERSION_RE.match(base)
        if m:
            out.append((int(m.group(1)), base))
    return sorted(out)


def check_numbering(names):
    """Versions must be unique and contiguous from 1 — refinery refuses duplicates, and a gap
    means a migration was lost in a merge (which reads as 'skipped' to every existing store)."""
    problems = []
    seen = {}
    for version, base in numbered(names):
        if version in seen:
            problems.append(f"version {version} is used twice: {seen[version]} and {base}")
        else:
            seen[version] = base
    if seen:
        missing = [v for v in range(1, max(seen) + 1) if v not in seen]
        if missing:
            problems.append(f"version number(s) {missing} unused, but V{max(seen)} exists")
    return problems


def out_of_order(base_names, head_names):
    """The pure half of the append check: which of `head_names` were added below `base_names`."""
    base_versions = numbered(base_names)
    if not base_versions:
        return []
    base_max = max(v for v, _ in base_versions)
    known = {b for _, b in base_versions}
    return [
        f"{base} was added at V{version}, at or below the existing highest V{base_max} — "
        "renumber it above that"
        for version, base in numbered(head_names)
        if base not in known and version <= base_max
    ]


def check_appended(base_ref, head_names):
    """A migration added on this branch must sit above every version that already existed.

    Returns a list of problems, or None when the comparison could not be made (no git, unknown
    base ref) — the caller says so out loud instead of counting it as a pass.
    """
    try:
        listing = subprocess.run(
            ["git", "ls-tree", "--name-only", base_ref, f"{MIGRATIONS_DIR}/"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.split()
    except (OSError, subprocess.CalledProcessError):
        return None
    return out_of_order(listing, head_names)


def self_test():
    ok = ["V1__init.sql", "V2__embeddings.sql", "V3__job_runs.sql"]
    assert check_numbering(ok) == [], "contiguous unique versions must pass"
    dupe = ["V1__init.sql", "V2__a.sql", "V2__b.sql"]
    assert any("twice" in p for p in check_numbering(dupe)), "must catch a repeated version"
    gap = ["V1__init.sql", "V3__job_runs.sql"]
    assert any("unused" in p for p in check_numbering(gap)), "must catch a missing version"

    # The 2026-08-09 slip itself: the base already carried V16, and V15 is added underneath it.
    slip = out_of_order(["V16__lessons.sql"], ["V15__briefs.sql", "V16__lessons.sql"])
    assert len(slip) == 1 and "V15__briefs.sql" in slip[0], f"must catch the V15-after-V16 slip: {slip}"
    appended = out_of_order(["V16__lessons.sql"], ["V16__lessons.sql", "V17__next.sql"])
    assert appended == [], "an ordinary appended migration must pass"
    assert out_of_order([], ["V1__init.sql"]) == [], "the first migration ever has nothing to sit above"

    clean = [
        ("V1__init.sql", "CREATE TABLE people (id INTEGER PRIMARY KEY);"),
        ("V3__add.sql", "ALTER TABLE people ADD COLUMN nickname TEXT; -- DROP COLUMN in a comment is fine"),
    ]
    dirty = [
        ("V4__bad.sql", "ALTER TABLE people DROP COLUMN confidence;"),
    ]
    assert scan(clean) == [], "additive DDL and commented keywords must pass"
    found = scan(dirty)
    assert len(found) == 1 and found[0][1] == "DROP COLUMN", "must catch a real DROP COLUMN"
    rebuild = "-- non-additive-ok: CHECK constraint rebuild\n-- ロールバック手順: ...\nDROP TABLE t;"
    assert scan([("V9__rebuild.sql", rebuild)]) == [], "a sanctioned rebuild with a rollback plan must pass"
    no_rollback = "-- non-additive-ok: CHECK constraint rebuild\nDROP TABLE t;"
    assert len(scan([("V9__bad.sql", no_rollback)])) == 1, "the exception mark alone (no rollback plan) must NOT pass"
    print(
        "self-test OK: detectors pass additive migrations and catch a DROP COLUMN, a repeated "
        "version, a version gap, and a migration inserted below the existing maximum."
    )


def migration_files():
    root = pathlib.Path(MIGRATIONS_DIR)
    if not root.exists():
        return []
    return [(p.as_posix(), p.read_text(encoding="utf-8", errors="replace")) for p in sorted(root.glob("*.sql"))]


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    files = migration_files()
    names = [path for path, _ in files]

    hits = scan(files)
    if hits:
        print("backward-compat violation: a migration contains a non-additive statement.")
        for path, label, snippet in hits:
            print(f"  - {path}: {label}  …{snippet}…")
        print("\nMigrations must be additive (memory lives for years). If a change is unavoidable, it needs a decision record + rollback plan (CLAUDE.md).")
        return 1

    numbering = check_numbering(names)
    if numbering:
        print("migration numbering violation: versions must be unique and contiguous from V1.")
        for problem in numbering:
            print(f"  - {problem}")
        return 1

    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    base_ref = args[0] if args else "origin/main"
    order = check_appended(base_ref, names)
    if order is None:
        order_note = f"append order NOT checked (base ref {base_ref!r} unavailable)"
    elif order:
        print(f"migration order violation (vs {base_ref}): a migration was inserted below existing ones.")
        for problem in order:
            print(f"  - {problem}")
        print(
            "\nrefinery hard-fails on a filesystem migration whose version is below the highest "
            "applied one, and the store then stops opening entirely. Renumber above the current "
            "maximum (this is the 2026-08-09 V15-after-V16 failure)."
        )
        return 1
    else:
        order_note = f"appended in order vs {base_ref}"

    print(f"migrations OK: {len(files)} file(s), all additive, V1..V{max(v for v, _ in numbered(names))}; {order_note}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
