#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# [tool.uv]
# dev-dependencies = ["pytest"]
# ///
"""Pre-commit hook: reject commits where the git author, committer, or co-author is an LLM.

An LLM cannot be held accountable for code contributions.
"""

from __future__ import annotations

import argparse
import re
import subprocess as sp
import sys
from dataclasses import dataclass

RE_CO_AUTHORED_BY = re.compile(r"^Co-Authored-By:\s*(?P<identity>.+)$", re.MULTILINE | re.IGNORECASE)
RE_IDENTITY = re.compile(r"^(?P<name>.+?)\s*<(?P<email>[^>]+)>.*$")
LLM_EMAILS = [
    "noreply@anthropic.com",
]


@dataclass(frozen=True)
class Identity:
    name: str
    email: str

    @classmethod
    def from_string(cls, s: str) -> Identity:
        match = RE_IDENTITY.match(s)
        if not match:
            raise ValueError(f"invalid identity string: {s}")
        return cls(name=match.group("name"), email=match.group("email"))


@dataclass(frozen=True)
class Contributors:
    author: Identity
    committer: Identity
    co_authors: set[Identity]


def run(argv):
    return sp.run(argv, stdout=sp.PIPE, stderr=sp.PIPE, text=True, check=False, timeout=5)


def git_author() -> str:
    return run(["git", "var", "GIT_AUTHOR_IDENT"]).stdout.strip()


def git_committer() -> str:
    return run(["git", "var", "GIT_COMMITTER_IDENT"]).stdout.strip()


def contributors(message: str, author: str | None = None, committer: str | None = None) -> Contributors:
    if author is None:
        author = git_author()
    if committer is None:
        committer = git_committer()

    author = Identity.from_string(author)
    committer = Identity.from_string(committer)

    co_authors = set()
    for line in RE_CO_AUTHORED_BY.finditer(message):
        co_authors.add(Identity.from_string(line.group(1).strip()))

    return Contributors(author=author, committer=committer, co_authors=co_authors)


def validate(contribs: Contributors) -> bool:
    result = True
    if contribs.author.email in LLM_EMAILS:
        print(f"git author email {contribs.author.email} is an LLM", file=sys.stderr)
        result = False
    if contribs.committer.email in LLM_EMAILS:
        print(f"git committer email {contribs.committer.email} is an LLM", file=sys.stderr)
        result = False
    for co_author in contribs.co_authors:
        if co_author.email in LLM_EMAILS:
            print(f"git co-author email {co_author.email} is an LLM", file=sys.stderr)
            result = False

    return result


def cli():
    parser = argparse.ArgumentParser(description="Ensure an LLM is not a code author.")
    parser.add_argument("message_file", help="Commit message file path")
    return parser


def main():
    args = cli().parse_args()
    with open(args.message_file, "r", encoding="utf-8") as f:
        message = f.read()
    contribs = contributors(message)

    ok = validate(contribs)
    if not ok:
        return 3


if __name__ == "__main__":
    sys.exit(main())

# Tests ================================================================================================================
# run manually with `uv run --with pytest pytest scripts/check-section-comments.py`


def test_validate_ok():
    assert validate(
        Contributors(
            author=Identity(name="Alice", email="alice@example.com"),
            committer=Identity(name="Bob", email="bob@example.com"),
            co_authors={Identity(name="Charlie", email="charlie@example.com")}
        )
    ) is True


def test_validate_llm_author():
    assert validate(
        Contributors(
            author=Identity(name="Alice", email="alice@example.com"),
            committer=Identity(name="Bob", email="bob@example.com"),
            co_authors={Identity(name="Claude Code", email="noreply@anthropic.com")}
        )
    ) is False
