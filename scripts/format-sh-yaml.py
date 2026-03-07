#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["ruamel.yaml"]
# [tool.uv]
# dev-dependencies = ["pytest"]
# ///
"""Enforce indentation in .sh.yaml test files.

Re-indents the YAML header (everything before the `---` separator) to use
two-space indentation with indented list items. All other formatting
(quoting, block scalars, key order) is preserved via ruamel.yaml round-trip.

Usage:
    uv run scripts/format-sh-yaml.py tests/**/*.sh.yaml
"""

import argparse
import io
import re
import sys
import traceback

from ruamel.yaml import YAML


def _make_yaml() -> YAML:
    yaml = YAML(typ="rt")
    yaml.preserve_quotes = True
    yaml.indent(mapping=2, sequence=4, offset=2)
    return yaml


def format_content(content: str) -> tuple[str, bool]:
    """Format a .sh.yaml string. Returns (result, changed)."""
    m = re.search(r"^---$", content, re.MULTILINE)
    if m is None:
        raise ValueError("content does not contain a '---' separator")
    header_text = content[:m.start()]
    body = content[m.start():]

    yaml = _make_yaml()
    header = yaml.load(header_text)

    if not isinstance(header, dict):
        raise ValueError("root element is not a mapping")

    buffer = io.StringIO()
    yaml.dump(header, buffer)
    buffer.write(body)
    result = buffer.getvalue()

    return result, result != content


def format_file(path: str) -> bool:
    """Format a single .sh.yaml file. Returns True if modified."""
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()

    result, changed = format_content(content)

    if changed:
        with open(path, "w", encoding="utf-8") as f:
            f.write(result)

    return changed


def main():
    parser = argparse.ArgumentParser(description="Enforce indentation in .sh.yaml test files.")
    parser.add_argument("files", nargs="+", metavar="FILE", help=".sh.yaml files to format")
    args = parser.parse_args()

    result = 0

    for path in args.files:
        print(path, end="... ", file=sys.stderr, flush=True)
        try:
            if format_file(path):
                print("reformatted", file=sys.stderr)
                if result != 1:  # Don't override a stronger error.
                    result = 3
            else:
                print("ok", file=sys.stderr)
        except Exception:
            print("error", file=sys.stderr)
            traceback.print_exc(file=sys.stderr)
            result = 1

    # 0: no changes
    # 1: unknown error
    # 2: CLI error
    # 3: some files reformatted
    return result


if __name__ == "__main__":
    sys.exit(main())

# Tests ================================================================================================================
# run manually with `uv run --with pytest pytest scripts/check-section-comments.py`


def test_already_formatted():
    content = "name: hello\n---\necho hello\n"
    result, changed = format_content(content)
    assert not changed
    assert result == content


def test_reindents_header():
    # 4-space indent should be fixed to 2-space.
    content = "cases:\n    -   name: foo\n---\necho foo\n"
    result, changed = format_content(content)
    assert changed
    assert "    -   name" not in result
    assert "  - name: foo" in result
    assert result.endswith("---\necho foo\n")


def test_body_preserved():
    body = "---\n#!/bin/bash\necho 'hello world'\nif true; then\n  echo yes\nfi\n"
    content = "name: test\n" + body
    result, changed = format_content(content)
    assert result.endswith(body)


def test_missing_separator():
    import pytest

    with pytest.raises(ValueError, match="separator"):
        format_content("name: test\necho hello\n")


def test_non_mapping_root():
    import pytest

    with pytest.raises(ValueError, match="mapping"):
        format_content("- item1\n- item2\n---\necho hello\n")


def test_preserves_quotes():
    content = 'name: "quoted value"\n---\necho hello\n'
    result, changed = format_content(content)
    assert '"quoted value"' in result


def test_preserves_block_scalar():
    content = "name: test\nstdout: |\n  line1\n  line2\n---\necho hello\n"
    result, changed = format_content(content)
    assert "|\n" in result
    assert "line1\n" in result


def test_idempotent():
    content = "cases:\n    -   name: foo\n---\necho foo\n"
    result1, _ = format_content(content)
    result2, changed2 = format_content(result1)
    assert not changed2
    assert result1 == result2
