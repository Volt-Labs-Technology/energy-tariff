#!/usr/bin/env python3
"""Fail if a public function is never exercised in #[cfg(test)] text.

Walks src/**/*.rs and collects (Type, fn) pairs from inherent impls
(`impl Type { pub fn` / `pub const fn`) plus free `pub fn` as (None, name).
Names are not unique-collapsed across types: Ratchet::new does not cover
Usd::new.

A test covers Type::name when comment-free test text contains `Type::name`,
or when a #[test] function calls `.name(` and also mentions Type. A comment
containing `foo()` does not cover `foo`. Free functions need `name(` in
comment-free test text.

Run from anywhere; paths are relative to the repo root.

PUBLIC_ITEMS_SELFTEST=1 runs an in-memory fixture that must fail to find a
test for Probe::new, then exits without scanning src/.
"""

from pathlib import Path
import os
import re
import sys

PUB_FN = re.compile(r"^\s*pub\s+(?:const\s+)?fn\s+(\w+)")
INHERENT_IMPL = re.compile(
    r"^\s*impl(?:<[^>]*>)?\s+(\w+)\s*(?:<[^>]*>)?\s*\{"
)
TYPE_WORD = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
ROOT = Path(__file__).resolve().parents[1]


def is_comment_line(line):
    """True when the line is `//` or `///` (or `//!`)."""
    return line.lstrip().startswith("//")


def strip_trailing_comment(line):
    """Drop a rustfmt trailing comment (` // ...`), keep `https://` intact."""
    index = line.find(" //")
    if index == -1:
        return line
    return line[:index]


def code_line(line):
    """Empty string for comment lines; otherwise the line without a trailing comment."""
    if is_comment_line(line):
        return ""
    return strip_trailing_comment(line)


def strip_comments(text):
    """Join non-comment lines, stripping trailing comments."""
    lines = []
    for line in text.splitlines():
        stripped = code_line(line)
        if stripped == "":
            continue
        lines.append(stripped)
    return "\n".join(lines)


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


def inherent_impl_type(line):
    """Type name for `impl Type {`, or None for trait impls and non-impl lines."""
    if re.search(r"\bfor\b", line):
        return None
    match = INHERENT_IMPL.match(line)
    if match is None:
        return None
    return match.group(1)


def public_items(production_lines):
    """(Type, fn) pairs in source order. Type is None for free functions."""
    items = []
    seen = set()
    depth = 0
    current_type = None
    for line in production_lines:
        if is_comment_line(line):
            continue
        impl_type = inherent_impl_type(line)
        if impl_type is not None:
            current_type = impl_type
        match = PUB_FN.match(line)
        if match is not None:
            item = (current_type, match.group(1))
            if item not in seen:
                seen.add(item)
                items.append(item)
        depth += line.count("{") - line.count("}")
        if depth <= 0:
            current_type = None
            depth = 0
    return items


def consume_block(lines, start):
    """(joined text, index after) for the brace block that starts at or after start."""
    i = start
    depth = 0
    opened = False
    block = []
    while i < len(lines):
        line = lines[i]
        block.append(line)
        if "{" in line:
            opened = True
        depth += line.count("{") - line.count("}")
        i += 1
        if opened and depth <= 0:
            return "\n".join(block), i
    return "\n".join(block), i


def test_function_bodies(test_text):
    """Bodies of `#[test]` functions in already comment-stripped test text."""
    lines = test_text.splitlines()
    bodies = []
    i = 0
    while i < len(lines):
        if lines[i].lstrip() != "#[test]":
            i += 1
            continue
        i += 1
        while i < len(lines):
            stripped = lines[i].lstrip()
            if stripped == "" or stripped.startswith("#["):
                i += 1
                continue
            break
        if i >= len(lines):
            break
        body, i = consume_block(lines, i)
        bodies.append(body)
    return bodies


def mentions_type(text, type_name):
    """True when `type_name` appears as a whole identifier."""
    for word in TYPE_WORD.findall(text):
        if word == type_name:
            return True
    return False


def is_covered(type_name, fn_name, test_text, test_fns):
    """Whether comment-free tests exercise this (Type, fn) pair."""
    if type_name is None:
        return f"{fn_name}(" in test_text
    if f"{type_name}::{fn_name}" in test_text:
        return True
    call = f".{fn_name}("
    for body in test_fns:
        if call in body and mentions_type(body, type_name):
            return True
    return False


def uncovered(items, test_text):
    """(Type, fn) pairs that tests do not cover."""
    code = strip_comments(test_text)
    test_fns = test_function_bodies(code)
    missing = []
    for type_name, fn_name in items:
        if not is_covered(type_name, fn_name, code, test_fns):
            missing.append((type_name, fn_name))
    return missing


def item_label(type_name, fn_name):
    """Printable `Type::name` or free-function name."""
    if type_name is None:
        return fn_name
    return f"{type_name}::{fn_name}"


def fail_selftest(message):
    """Print a self-check failure and return 1."""
    print(f"PUBLIC_ITEMS_SELFTEST failed: {message}")
    return 1


def run_selftest():
    """In-memory fixtures that the scanner must reject or accept as specified."""
    probe_src = "\n".join(
        [
            "impl Probe {",
            "    pub fn new() {}",
            "}",
        ]
    )
    probe_items = public_items(probe_src.splitlines())
    if ("Probe", "new") not in probe_items:
        return fail_selftest("expected to collect Probe::new")
    probe_missing = uncovered(probe_items, "")
    if ("Probe", "new") not in probe_missing:
        return fail_selftest("Probe::new with no test must be uncovered")

    both_src = "\n".join(
        [
            "impl Usd {",
            "    pub fn new() {}",
            "    pub fn get() {}",
            "}",
            "impl Ratchet {",
            "    pub fn new() {}",
            "}",
            "pub fn foo() {}",
        ]
    )
    both_items = public_items(both_src.splitlines())
    expected = [("Usd", "new"), ("Usd", "get"), ("Ratchet", "new"), (None, "foo")]
    if both_items != expected:
        return fail_selftest(f"expected {expected}, got {both_items}")

    ratchet_only = "\n".join(
        [
            "#[cfg(test)]",
            "mod tests {",
            "    #[test]",
            "    fn covers_ratchet_only() {",
            "        Ratchet::new();",
            "    }",
            "}",
        ]
    )
    missing = uncovered(both_items, ratchet_only)
    if ("Usd", "new") not in missing:
        return fail_selftest("Ratchet::new must not cover Usd::new")
    if ("Ratchet", "new") in missing:
        return fail_selftest("Ratchet::new should be covered by Ratchet::new()")
    if (None, "foo") not in missing:
        return fail_selftest("foo with no call must be uncovered")

    comment_only = "\n".join(
        [
            "#[cfg(test)]",
            "mod tests {",
            "    #[test]",
            "    fn comments_do_not_count() {",
            "        // foo()",
            "        /// Usd::get",
            "        let _ = 1; // foo()",
            "    }",
            "}",
        ]
    )
    missing = uncovered(both_items, comment_only)
    if (None, "foo") not in missing:
        return fail_selftest("a comment containing foo() must not cover foo")
    if ("Usd", "get") not in missing:
        return fail_selftest("a comment containing Usd::get must not cover Usd::get")

    method_call = "\n".join(
        [
            "#[cfg(test)]",
            "mod tests {",
            "    #[test]",
            "    fn usd_get_via_method() {",
            "        let amount = Usd::new(1.0);",
            "        let _ = amount.get();",
            "    }",
            "}",
        ]
    )
    missing = uncovered(both_items, method_call)
    if ("Usd", "get") in missing:
        return fail_selftest(".get( in a test that mentions Usd must cover Usd::get")
    if ("Usd", "new") in missing:
        return fail_selftest("Usd::new in a test must cover Usd::new")

    other_get = "\n".join(
        [
            "#[cfg(test)]",
            "mod tests {",
            "    #[test]",
            "    fn window_get_is_not_usd_get() {",
            "        let _ = window.get();",
            "    }",
            "}",
        ]
    )
    missing = uncovered(both_items, other_get)
    if ("Usd", "get") not in missing:
        return fail_selftest(".get( without mentioning Usd must not cover Usd::get")

    commented_out_fn = "\n".join(
        [
            "// pub fn hidden() {}",
            "impl Probe {",
            "    pub fn new() {}",
            "}",
        ]
    )
    items = public_items(commented_out_fn.splitlines())
    if (None, "hidden") in items:
        return fail_selftest("commented-out pub fn must not be collected")

    print("PUBLIC_ITEMS_SELFTEST ok")
    return 0


def scan_src(src):
    """Uncovered (Type, fn) pairs across src/**/*.rs."""
    items = []
    seen = set()
    test_chunks = []
    for path in sorted(src.rglob("*.rs")):
        production, test_text = split_production_and_test(
            path.read_text(encoding="utf-8")
        )
        test_chunks.append(test_text)
        for item in public_items(production):
            if item not in seen:
                seen.add(item)
                items.append(item)
    return uncovered(items, "\n".join(test_chunks))


def main():
    if os.environ.get("PUBLIC_ITEMS_SELFTEST") == "1":
        return run_selftest()

    src = ROOT / "src"
    if not src.is_dir():
        print("src/ is missing")
        return 1

    missing = scan_src(src)
    if missing:
        print(
            "public functions must appear in #[cfg(test)] as Type::name, "
            "or .name( in a #[test] whose body mentions Type"
        )
        for type_name, fn_name in missing:
            print(item_label(type_name, fn_name))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
