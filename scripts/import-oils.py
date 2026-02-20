#!/usr/bin/env python3
# /// script
# dependencies = ["ruamel.yaml"]
# ///
"""Import Oils spec tests into thaum corpus format.

Clones the Oils repository at a pinned revision, parses spec/*.test.sh files,
and converts each test case into a .sh.yaml corpus file.

Usage:
    uv run scripts/import-oils.py
    uv run scripts/import-oils.py --rev abc123
    uv run scripts/import-oils.py --spec-files 'arith.test.sh,loop.test.sh'
"""

import argparse
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from ruamel.yaml import YAML

OILS_REPO = "https://github.com/oils-for-unix/oils.git"
DEFAULT_REV = "master"

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent
DEFAULT_OUTPUT_DIR = PROJECT_ROOT / "tests" / "corpus" / "oils"


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclass
class TestCase:
    name: str
    code_lines: list[str] = field(default_factory=list)
    stdout: str | None = None
    stderr: str | None = None
    status: int | None = None
    # Parser state for multi-line blocks
    _collecting: str | None = field(default=None, repr=False)
    _collect_buf: list[str] = field(default_factory=list, repr=False)
    _skip_until_end: bool = field(default=False, repr=False)

    @property
    def slug(self) -> str:
        """Slugified name for use as filename."""
        s = self.name.lower()
        s = re.sub(r"[^a-z0-9\s-]", "", s)
        s = re.sub(r"[\s]+", "-", s).strip("-")
        return s or "unnamed"

    @property
    def code(self) -> str:
        return "\n".join(self.code_lines)


# ---------------------------------------------------------------------------
# Oils spec file parser
# ---------------------------------------------------------------------------

def parse_spec_file(path: Path) -> list[TestCase]:
    """Parse an Oils spec test file into a list of TestCases."""
    tests: list[TestCase] = []
    current: TestCase | None = None

    for line in path.read_text().splitlines():
        # New test case
        if line.startswith("#### "):
            if current is not None:
                _finalize(current)
                tests.append(current)
            current = TestCase(name=line[5:].strip())
            continue

        # Preamble (before first ####)
        if current is None:
            continue

        # Inside a multi-line block being skipped (N-I, OK, BUG)
        if current._skip_until_end:
            if line.strip() == "## END":
                current._skip_until_end = False
            continue

        # Inside a multi-line STDOUT:/STDERR: block
        if current._collecting is not None:
            if line.strip() == "## END":
                value = "\n".join(current._collect_buf) + "\n"
                _set_output(current, current._collecting, value)
                current._collecting = None
                current._collect_buf = []
            else:
                current._collect_buf.append(line)
            continue

        # Annotation lines
        if line.startswith("## "):
            annotation = line[3:]

            # Shell-specific variants — skip
            if annotation.startswith(("N-I ", "OK ", "BUG ", "BUG-2 ")):
                # Check if this starts a multi-line block
                if "STDOUT:" in annotation or "STDERR:" in annotation:
                    current._skip_until_end = True
                continue

            # Single-line stdout/stderr
            if annotation.startswith("stdout: "):
                current.stdout = annotation[8:] + "\n"
                continue
            if annotation.startswith("stderr: "):
                current.stderr = annotation[8:] + "\n"
                continue

            # JSON stdout/stderr
            if annotation.startswith("stdout-json: "):
                current.stdout = json.loads(annotation[13:])
                continue
            if annotation.startswith("stderr-json: "):
                current.stderr = json.loads(annotation[13:])
                continue

            # Multi-line STDOUT/STDERR blocks
            if annotation == "STDOUT:":
                current._collecting = "stdout"
                current._collect_buf = []
                continue
            if annotation == "STDERR:":
                current._collecting = "stderr"
                current._collect_buf = []
                continue

            # Status
            if annotation.startswith("status: "):
                current.status = int(annotation[8:])
                continue

            # Other annotations (compare_shells, oils_failures_allowed, etc.) — skip
            continue

        # Regular shell code
        current.code_lines.append(line)

    # Don't forget the last test
    if current is not None:
        _finalize(current)
        tests.append(current)

    return tests


def _finalize(test: TestCase):
    """Clean up a test case after parsing."""
    # Strip trailing blank lines from code
    while test.code_lines and not test.code_lines[-1].strip():
        test.code_lines.pop()


def _set_output(test: TestCase, field: str, value: str):
    """Set stdout or stderr on a test case."""
    if field == "stdout":
        test.stdout = value
    elif field == "stderr":
        test.stderr = value


# ---------------------------------------------------------------------------
# YAML emission
# ---------------------------------------------------------------------------

def emit_test_file(test: TestCase, topic: str) -> str:
    """Generate .sh.yaml content for a test case."""
    yaml = YAML()
    yaml.default_flow_style = False

    header: dict = {}
    header["name"] = test.name
    header["dialect"] = "bash"
    header["source"] = f"oils:spec/{topic}#{test.slug}"
    # is-valid defaults to true, so we omit it
    if test.status is not None:
        header["status"] = test.status
    if test.stdout is not None:
        header["stdout"] = test.stdout
    if test.stderr is not None:
        header["stderr"] = test.stderr

    buf = io.StringIO()
    yaml.dump(header, buf)
    yaml_str = buf.getvalue()

    return yaml_str + "---\n" + test.code + "\n"


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def clone_oils(rev: str) -> Path:
    """Clone Oils into a fresh temp directory and return its path."""
    tmpdir = Path(tempfile.mkdtemp(prefix="oils-import-"))
    print(f"Cloning Oils ({rev}) into {tmpdir} ...")
    subprocess.run(
        ["git", "clone", "--depth", "1", "--branch", rev, OILS_REPO, str(tmpdir / "oils")],
        check=True,
        capture_output=True,
    )
    return tmpdir / "oils"


def import_spec_files(
    clone_dir: Path,
    output_dir: Path,
    spec_filter: list[str] | None,
):
    """Parse and convert all spec files."""
    spec_dir = clone_dir / "spec"
    if not spec_dir.is_dir():
        print(f"error: {spec_dir} not found", file=sys.stderr)
        sys.exit(1)

    spec_files = sorted(spec_dir.glob("*.test.sh"))
    if spec_filter:
        allowed = set(spec_filter)
        spec_files = [f for f in spec_files if f.name in allowed]

    total_tests = 0
    total_files = 0
    topics = set()

    for spec_file in spec_files:
        topic = spec_file.stem.replace(".test", "").rstrip("_")
        topics.add(topic)

        tests = parse_spec_file(spec_file)
        if not tests:
            continue

        topic_dir = output_dir / topic
        topic_dir.mkdir(parents=True, exist_ok=True)

        # Track slugs to handle duplicates
        slug_counts: dict[str, int] = {}

        for test in tests:
            slug = test.slug
            if not slug:
                continue

            # Handle duplicate slugs
            if slug in slug_counts:
                slug_counts[slug] += 1
                slug = f"{slug}-{slug_counts[slug]}"
            else:
                slug_counts[slug] = 0

            filename = f"{slug}.sh.yaml"
            content = emit_test_file(test, topic)
            (topic_dir / filename).write_text(content)
            total_files += 1

        total_tests += len(tests)

    return total_tests, total_files, len(topics)


def main():
    parser = argparse.ArgumentParser(description="Import Oils spec tests into thaum corpus format")
    parser.add_argument("--rev", default=DEFAULT_REV, help=f"Oils revision to clone (default: {DEFAULT_REV})")
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR, help="Output directory")
    parser.add_argument("--spec-files", help="Comma-separated list of spec files to import (e.g. 'arith.test.sh,loop.test.sh')")
    args = parser.parse_args()

    spec_filter = args.spec_files.split(",") if args.spec_files else None

    # Clone
    clone_dir = clone_oils(args.rev)

    try:
        # Clean output dir
        if args.output_dir.exists():
            shutil.rmtree(args.output_dir)

        # Import
        total_tests, total_files, total_topics = import_spec_files(
            clone_dir, args.output_dir, spec_filter
        )

        print(f"\nDone: {total_files} files from {total_tests} tests across {total_topics} topics")
        print(f"Output: {args.output_dir}")

    finally:
        # Clean up clone
        shutil.rmtree(clone_dir.parent, ignore_errors=True)


if __name__ == "__main__":
    main()
