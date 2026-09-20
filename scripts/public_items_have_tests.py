#!/usr/bin/env python3
"""Fail if a public function is never mentioned in #[cfg(test)] text.

Walks src/**/*.rs, collects `pub fn` / `pub const fn` names from production
code, and requires each name to appear in #[cfg(test)] module text as
`name(` or `::name`. Run from anywhere; paths are relative to the repo root.
"""

from pathlib import Path
import re
import sys

PUB_FN = re.compile(r"^\s*pub\s+(?:const\s+)?fn\s+(\w+)")
ROOT = Path(__file__).resolve().parents[1]


def skip_cfg_test_item(lines, index):
    """Index after the item annotated by #[cfg(test)] at index."""
    i = index + 1
    while i < len(lines):
        stripped = lines[i].lstrip()
        if stripped == "" or stripped.startswith("//") or stripped.startswith("#["):
            i += 1
            continue
        break
    depth = 0
    opened = False
    while i < len(lines):
        line = lines[i]
        if "{" in line:
            opened = True
        depth += line.count("{") - line.count("}")
        i += 1
        if opened:
            if depth <= 0:
                return i
        elif ";" in line:
            return i
    return i


def split_production_and_test(text):
    """Production lines and concatenated #[cfg(test)] item text."""
    lines = text.splitlines()
    production = []
    test_chunks = []
    i = 0
    while i < len(lines):
        if lines[i].lstrip().startswith("#[cfg(test)]"):
            end = skip_cfg_test_item(lines, i)
            test_chunks.append("\n".join(lines[i:end]))
            i = end
            continue
        production.append(lines[i])
        i += 1
    return production, "\n".join(test_chunks)


def public_fn_names(production_lines):
    """Unique `pub fn` / `pub const fn` names, source order."""
    names = []
    seen = set()
    for line in production_lines:
        if line.lstrip().startswith("//"):
            continue
        match = PUB_FN.match(line)
        if match is None:
            continue
        name = match.group(1)
        if name not in seen:
            seen.add(name)
            names.append(name)
    return names


def uncovered(names, test_text):
    """Names that never appear as `name(` or `::name` in test text."""
    missing = []
    for name in names:
        if f"{name}(" not in test_text and f"::{name}" not in test_text:
            missing.append(name)
    return missing


def main():
    src = ROOT / "src"
    if not src.is_dir():
        print("src/ is missing")
        return 1

    names = []
    seen = set()
    test_chunks = []
    for path in sorted(src.rglob("*.rs")):
        production, test_text = split_production_and_test(
            path.read_text(encoding="utf-8")
        )
        test_chunks.append(test_text)
        for name in public_fn_names(production):
            if name not in seen:
                seen.add(name)
                names.append(name)

    missing = uncovered(names, "\n".join(test_chunks))
    if missing:
        print("public functions must appear in #[cfg(test)] text as name( or ::name")
        print("\n".join(missing))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
