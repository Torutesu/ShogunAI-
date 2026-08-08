#!/usr/bin/env python3
"""Schema backward-compatibility guard (CLAUDE.md: memory lives for years; never break compat).

Migrations must be additive. A DROP COLUMN / DROP TABLE / RENAME / ALTER COLUMN in a migration
breaks readers of older data and corrupts a memory that is meant to survive for years. This check
scans the refinery migration SQL and fails on any backward-incompatible statement.

Run from the repo root:  python3 scripts/check-migrations.py
Self-test the detector:   python3 scripts/check-migrations.py --self-test
"""

import pathlib
import re
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


def self_test():
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
    print("self-test OK: detector passes additive migrations and catches a DROP COLUMN.")


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
    hits = scan(files)
    if hits:
        print("backward-compat violation: a migration contains a non-additive statement.")
        for path, label, snippet in hits:
            print(f"  - {path}: {label}  …{snippet}…")
        print("\nMigrations must be additive (memory lives for years). If a change is unavoidable, it needs a decision record + rollback plan (CLAUDE.md).")
        return 1
    print(f"migrations OK: {len(files)} file(s), all additive.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
