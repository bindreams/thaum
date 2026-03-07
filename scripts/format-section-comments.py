#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# [tool.uv]
# dev-dependencies = ["pytest"]
# ///
"""Pre-commit hook: fix section comment formatting in Rust files.

Section comments must follow the format:

    // Section name ======...=  (primary, filled with '=' to column 120)
    // Section name ------...-  (secondary, filled with '-' to column 120)

Leading whitespace counts toward the column limit.

Also detects section comments broken across two lines by comment reflow:

    // Section name was too long
    // -------------------------

These are merged back into a single correctly-formatted line.
"""

import re
import sys

COLUMN_LIMIT = 120

# Single-line: // <text> <run of = or ->  (at least 5 fill chars).
SECTION_RE = re.compile(r"^(\s*)//\s+.+\s+([=-])\2{4,}\s*$")

# Canonical format for extraction: // <name> <fill>.
CANONICAL_RE = re.compile(r"^(\s*)// (.+?) ([=-])\3{4,}\s*$")

# Fill-only line: // <run of = or ->  (at least 5 fill chars, no other text).
FILL_ONLY_RE = re.compile(r"^(\s*)//\s+([=-])\2{4,}\s*$")

# Preceding comment line that could be the name half of a broken section comment.
# Must be a plain // comment with text, NOT already a section comment, NOT a doc
# comment (///), and NOT a module doc comment (//!).
NAME_HALF_RE = re.compile(r"^(\s*)//(?: (.+))?\s*$")


def rebuild(indent: str, name: str, fill_char: str) -> str:
    """Build a canonical section comment line at COLUMN_LIMIT width."""
    prefix = f"{indent}// {name} "
    fill_count = COLUMN_LIMIT - len(prefix)
    if fill_count < 5:
        fill_count = 5
    return prefix + fill_char * fill_count


def process_lines(lines: list[str]) -> tuple[list[str], bool]:
    """Fix section comments in a list of lines. Returns (new_lines, changed)."""
    changed = False
    skip_next = False
    new_lines: list[str] = []

    for i, raw_line in enumerate(lines):
        if skip_next:
            skip_next = False
            continue

        line = raw_line.rstrip("\n")

        # Case 1: fill-only line — check if previous line is the name half.
        fill_m = FILL_ONLY_RE.match(line)
        if fill_m and new_lines:
            prev_line = new_lines[-1].rstrip("\n")
            name_m = NAME_HALF_RE.match(prev_line)
            if (
                name_m and not SECTION_RE.match(prev_line) and not prev_line.lstrip().startswith("///")
                and not prev_line.lstrip().startswith("//!") and name_m.group(2)  # has actual text after //
            ):
                indent = name_m.group(1)
                name = name_m.group(2).rstrip()
                fill_char = fill_m.group(2)
                fixed = rebuild(indent, name, fill_char)
                new_lines[-1] = fixed + "\n"
                changed = True
                continue

        # Case 2: single-line section comment with wrong format/length.
        sec_m = SECTION_RE.match(line)
        if sec_m:
            can_m = CANONICAL_RE.match(line)
            if can_m:
                indent, name, fill_char = can_m.group(1), can_m.group(2), can_m.group(3)
                fixed = rebuild(indent, name, fill_char)
                if fixed != line:
                    new_lines.append(fixed + "\n")
                    changed = True
                    continue

        # Case 3: two-line break where name is on current line and fill is on next.
        if i + 1 < len(lines):
            next_line = lines[i + 1].rstrip("\n")
            fill_m2 = FILL_ONLY_RE.match(next_line)
            if fill_m2:
                name_m2 = NAME_HALF_RE.match(line)
                if (
                    name_m2 and not SECTION_RE.match(line) and not line.lstrip().startswith("///")
                    and not line.lstrip().startswith("//!") and name_m2.group(2)
                ):
                    indent = name_m2.group(1)
                    name = name_m2.group(2).rstrip()
                    fill_char = fill_m2.group(2)
                    fixed = rebuild(indent, name, fill_char)
                    new_lines.append(fixed + "\n")
                    skip_next = True
                    changed = True
                    continue

        new_lines.append(raw_line)

    return new_lines, changed


def process_file(path: str) -> bool:
    """Process one file. Returns True if the file was modified."""
    with open(path) as f:
        lines = f.readlines()

    new_lines, changed = process_lines(lines)

    if changed:
        with open(path, "w") as f:
            f.writelines(new_lines)

    return changed


def main():
    any_changed = False
    for path in sys.argv[1:]:
        if process_file(path):
            print(f"Fixed section comments in {path}", file=sys.stderr)
            any_changed = True

    if any_changed:
        return 1  # Signal to pre-commit that changes were made.


if __name__ == "__main__":
    sys.exit(main())

# Tests ================================================================================================================
# run manually with `uv run --with pytest pytest scripts/check-section-comments.py`


def _lines(text: str) -> list[str]:
    """Split text into lines preserving newlines, like file.readlines()."""
    return [line + "\n" for line in text.split("\n")
            if line or text.endswith("\n")][:text.count("\n") + (1 if not text.endswith("\n") else 0)]


def _fix(text: str) -> str:
    result, _ = process_lines(_lines(text))
    return "".join(result)


def _changed(text: str) -> bool:
    _, changed = process_lines(_lines(text))
    return changed


_CORRECT_PRIMARY = "// CLI " + "=" * 113
_CORRECT_SECONDARY = "    // Helpers " + "-" * (120 - len("    // Helpers "))


def test_correct_comment_unchanged():
    assert not _changed(_CORRECT_PRIMARY)
    assert not _changed(_CORRECT_SECONDARY)


def test_too_short_padded():
    short = "// CLI " + "=" * 30
    result = _fix(short)
    assert result.rstrip() == _CORRECT_PRIMARY
    assert len(result.rstrip()) == 120


def test_too_long_trimmed():
    long = "// CLI " + "=" * 200
    result = _fix(long)
    assert result.rstrip() == _CORRECT_PRIMARY


def test_indented_comment():
    indented = "    // Helpers " + "-" * 50
    result = _fix(indented)
    assert result.rstrip() == _CORRECT_SECONDARY
    assert len(result.rstrip()) == 120


def test_reflow_name_then_fill():
    text = "// Section name was reflowed\n// -------------------------\n"
    result = _fix(text)
    assert result.count("\n") == 1
    line = result.rstrip()
    assert line.startswith("// Section name was reflowed -")
    assert len(line) == 120


def test_reflow_fill_after_previous_output():
    text = "fn foo() {}\n// Broken section\n// ==========\nfn bar() {}\n"
    result = _fix(text)
    lines = result.splitlines()
    assert lines[0] == "fn foo() {}"
    assert lines[1].startswith("// Broken section =")
    assert len(lines[1]) == 120
    assert lines[2] == "fn bar() {}"


def test_doc_comment_not_touched():
    text = "/// This is a doc comment\n// ==========\n"
    assert not _changed(text)


def test_module_doc_comment_not_touched():
    text = "//! Module doc\n// ==========\n"
    assert not _changed(text)


def test_plain_comment_not_touched():
    text = "// This is a regular comment, not a section\nfn foo() {}\n"
    assert not _changed(text)


def test_code_not_touched():
    text = 'fn main() {\n    println!("hello");\n}\n'
    assert not _changed(text)


def test_indented_reflow():
    text = "    // Indented section\n    // -------------------\n"
    result = _fix(text)
    assert result.count("\n") == 1
    line = result.rstrip()
    assert line.startswith("    // Indented section -")
    assert len(line) == 120


def test_indented_reflow_equals():
    text = "        // Deep nesting\n        // ============\n"
    result = _fix(text)
    assert result.count("\n") == 1
    line = result.rstrip()
    assert line.startswith("        // Deep nesting =")
    assert len(line) == 120


def test_indented_correct_unchanged():
    indent = "        "
    name = "Deep nesting"
    line = f"{indent}// {name} " + "=" * (120 - len(f"{indent}// {name} "))
    assert len(line) == 120
    assert not _changed(line)


def test_very_long_name_gets_minimum_fill():
    name = "A" * 115
    text = f"// {name} =====\n"
    result = _fix(text)
    line = result.rstrip()
    assert line.endswith("=" * 5)
    assert f"// {name} " in line
