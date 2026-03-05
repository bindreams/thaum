#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["ruamel.yaml"]
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


def format_file(path: str) -> bool:
    """Format a single .sh.yaml file. Returns True if modified."""
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()

    # Split at the first ^---$ line.  Everything before (including the
    # trailing \n) is the YAML header; the --- and everything after is the
    # shell-script body.
    m = re.search(r"^---$", content, re.MULTILINE)
    if m is None:
        raise ValueError(f"{path} does not contain a '---' separator")
    header_text = content[: m.start()]  # includes trailing \n
    body = content[m.start() :]  # starts with "---\n..."

    yaml = YAML(typ="rt")
    yaml.preserve_quotes = True
    yaml.indent(mapping=2, sequence=4, offset=2)
    header = yaml.load(header_text)

    if not isinstance(header, dict):
        raise ValueError(f"{path}: root element is not a mapping")

    # Round-trip the header through ruamel.yaml and reassemble the file.
    buffer = io.StringIO()
    yaml.dump(header, buffer)
    buffer.write(body)
    result = buffer.getvalue()

    if result == content:
        return False

    with open(path, "w", encoding="utf-8") as f:
        f.write(result)

    return True


def main():
    parser = argparse.ArgumentParser(
        description="Enforce indentation in .sh.yaml test files."
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
