#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["ruamel.yaml"]
# ///
"""Format .sh.yaml test files.

Converts stdout/stderr fields from quoted strings with escape sequences
to YAML block scalars (|) for readability. Uses ruamel.yaml round-trip
mode to preserve key order, comments, and quoting of unmodified fields.

Usage:
    uv run scripts/format-sh-yaml.py tests/**/*.sh.yaml
"""

import argparse
import io
import sys
import traceback

from ruamel.yaml import YAML
from ruamel.yaml.scalarstring import LiteralScalarString

# Control characters that are NOT allowed in YAML block scalars.
# Block scalars permit TAB (0x09), LF (0x0A), CR (0x0D), and NEL (0x85).
# All other C0/C1 controls must stay in double-quoted strings with escapes.
_BLOCK_SCALAR_FORBIDDEN = set()
_BLOCK_SCALAR_FORBIDDEN |= {chr(c) for c in range(0x00, 0x09)}  # NUL..BS
_BLOCK_SCALAR_FORBIDDEN |= {chr(c) for c in range(0x0B, 0x0D)}  # VT, FF
_BLOCK_SCALAR_FORBIDDEN |= {chr(c) for c in range(0x0E, 0x20)}  # SO..US
_BLOCK_SCALAR_FORBIDDEN.add(chr(0x7F))  # DEL


def _needs_quoting(value: str) -> bool:
    """Return True if the value contains characters that can't be in a block scalar."""
    return bool(_BLOCK_SCALAR_FORBIDDEN.intersection(value))


def format_file(path: str) -> bool:
    """Format a single .sh.yaml file. Returns True if modified."""
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()

    if "\n---\n" not in content:
        raise ValueError(f"{path} does not contain a '---' separator")
    header_text, body = content.split("\n---\n", 1)

    yaml = YAML(typ="rt")
    yaml.preserve_quotes = True
    header = yaml.load(header_text)

    if not isinstance(header, dict):
        raise ValueError(f"{path}: root element is not a mapping")

    for field in ("stdout", "stderr"):
        value = header.get(field)
        if not isinstance(value, str):
            continue
        if "\n" not in value:
            continue
        # Block scalar | can't losslessly represent whitespace-only values.
        if not value.strip():
            continue
        # Already a LiteralScalarString — no conversion needed.
        if isinstance(value, LiteralScalarString):
            continue
        # Values with control characters must stay double-quoted.
        if _needs_quoting(value):
            continue

        header[field] = LiteralScalarString(value)

    # Re-create all block scalars from their plain string value to discard
    # ruamel.yaml's round-trip metadata (stale chomping indicators). This
    # ensures the dumper picks |/|- based on the actual value content.
    for key in list(header.keys()):
        value = header[key]
        if isinstance(value, LiteralScalarString):
            header[key] = LiteralScalarString(str(value))

    # Round-trip the header through ruamel.yaml and reassemble the file.
    buffer = io.StringIO()
    yaml.dump(header, buffer)
    buffer.write("---\n")
    buffer.write(body)
    result = buffer.getvalue()

    if result == content:
        return False

    with open(path, "w", encoding="utf-8") as f:
        f.write(result)

    return True


def main():
    parser = argparse.ArgumentParser(
        description="Format .sh.yaml test files: convert stdout/stderr to block scalars."
    )
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
