#!/usr/bin/env python
# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License"). You
# may not use this file except in compliance with the License. A copy of
# the License is located at
#
#     http://aws.amazon.com/apache2.0/
#
# or in the "license" file accompanying this file. This file is
# distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF
# ANY KIND, either express or implied. See the License for the specific
# language governing permissions and limitations under the License.

"""Sync the Rust workspace version in Cargo.toml with the Python
``_version.py`` source of truth.

The Python side is authoritative because Braket CI expects
``<package>/_version.py`` to be the single place versions live. The
Rust side needs the same number in ``Cargo.toml``'s
``[workspace.package].version`` so that ``cargo build`` and ``maturin
build`` agree on the wheel metadata. This script copies the value
across.

Usage:

    # Rewrite Cargo.toml to match _version.py (normal developer workflow).
    python scripts/sync_version.py

    # Fail if they're out of sync without rewriting (CI gate).
    python scripts/sync_version.py --check

PEP 440 ↔ Cargo semver translation: PEP 440 allows ``0.1.0.dev0`` /
``0.1.0a1`` / ``0.1.0rc2`` / ``0.1.0.post1``; Cargo semver requires
``MAJOR.MINOR.PATCH`` with an optional pre-release tag after a single
hyphen (``0.1.0-dev0``, ``0.1.0-alpha.1``, ``0.1.0-rc.2``). ``.postN``
releases are not expressible in semver; they flatten to the bare
``MAJOR.MINOR.PATCH`` with a Cargo build-metadata suffix
(``0.1.0+post1``). The mapping is lossless in both directions for the
forms we actually use.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
VERSION_PY = REPO_ROOT / "python" / "qirtoqasm" / "_version.py"
CARGO_TOML = REPO_ROOT / "Cargo.toml"
CARGO_LOCK = REPO_ROOT / "Cargo.lock"

_WORKSPACE_CRATES = ("qirtoqasm-core", "qirtoqasm-ffi", "qirtoqasm-py")
_VERSION_PY_RE = re.compile(r'^__version__\s*=\s*"([^"]+)"\s*$', re.MULTILINE)
_CARGO_VERSION_RE = re.compile(
    r'(?m)^(\[workspace\.package\][^\[]*?^version\s*=\s*)"([^"]+)"',
)


def read_python_version() -> str:
    text = VERSION_PY.read_text()
    match = _VERSION_PY_RE.search(text)
    if not match:
        raise SystemExit(f"Could not find __version__ in {VERSION_PY}")
    return match.group(1)


def read_cargo_version() -> str:
    text = CARGO_TOML.read_text()
    match = _CARGO_VERSION_RE.search(text)
    if not match:
        raise SystemExit(f"Could not find [workspace.package].version in {CARGO_TOML}")
    return match.group(2)


def pep440_to_cargo(pep440: str) -> str:
    """Translate a PEP 440 version string to a Cargo-compatible one.

    Examples:
      ``0.1.0`` → ``0.1.0``
      ``0.1.0.dev0`` → ``0.1.0-dev0``
      ``0.1.0a1`` → ``0.1.0-alpha.1``
      ``0.1.0rc2`` → ``0.1.0-rc.2``
      ``0.1.0.post1`` → ``0.1.0+post1``  (build metadata, not pre-release)
    """
    m = re.match(
        r"^(?P<base>\d+\.\d+\.\d+)"
        r"(?:(?P<pre_kind>a|b|rc|alpha|beta)(?P<pre_n>\d+))?"
        r"(?:\.dev(?P<dev_n>\d+))?"
        r"(?:\.post(?P<post_n>\d+))?"
        r"$",
        pep440,
    )
    if not m:
        raise SystemExit(f"Unrecognized PEP 440 version: {pep440!r}")
    base = m.group("base")
    pre_kind = m.group("pre_kind")
    pre_n = m.group("pre_n")
    dev_n = m.group("dev_n")
    post_n = m.group("post_n")

    tags: list[str] = []
    if pre_kind is not None:
        kind_map = {"a": "alpha", "b": "beta", "rc": "rc", "alpha": "alpha", "beta": "beta"}
        tags.append(f"{kind_map[pre_kind]}.{pre_n}")
    if dev_n is not None:
        tags.append(f"dev{dev_n}")

    cargo = base
    if tags:
        cargo += "-" + ".".join(tags)
    if post_n is not None:
        # semver reserves the segment after `+` for build metadata; this is
        # fine for Cargo's purposes and keeps the mapping round-trippable.
        cargo += f"+post{post_n}"
    return cargo


def rewrite_cargo_version(new_version: str) -> bool:
    """Rewrite ``Cargo.toml`` to ``new_version``. Return True if changed."""
    text = CARGO_TOML.read_text()
    match = _CARGO_VERSION_RE.search(text)
    if not match:
        raise SystemExit(f"Could not find [workspace.package].version in {CARGO_TOML}")
    if match.group(2) == new_version:
        return False
    new_text = text[: match.start()] + match.group(1) + f'"{new_version}"' + text[match.end() :]
    CARGO_TOML.write_text(new_text)
    return True


def _cargo_lock_entry_re(crate: str) -> re.Pattern[str]:
    """Match a specific workspace crate's ``[[package]]`` block in Cargo.lock,
    capturing the ``version = "..."`` line inside it."""
    return re.compile(
        rf'(\[\[package\]\]\s*\nname\s*=\s*"{re.escape(crate)}"\s*\nversion\s*=\s*)'
        r'"([^"]+)"',
    )


def read_cargo_lock_versions() -> dict[str, str]:
    """Return the current version pinned in Cargo.lock for each workspace crate."""
    text = CARGO_LOCK.read_text()
    versions: dict[str, str] = {}
    for crate in _WORKSPACE_CRATES:
        match = _cargo_lock_entry_re(crate).search(text)
        if not match:
            raise SystemExit(f"Could not find [[package]] entry for {crate} in {CARGO_LOCK}")
        versions[crate] = match.group(2)
    return versions


def rewrite_cargo_lock(new_version: str) -> bool:
    """Rewrite each workspace crate's version in Cargo.lock. Return True if any changed."""
    text = CARGO_LOCK.read_text()
    changed = False
    for crate in _WORKSPACE_CRATES:
        pattern = _cargo_lock_entry_re(crate)
        match = pattern.search(text)
        if not match:
            raise SystemExit(f"Could not find [[package]] entry for {crate} in {CARGO_LOCK}")
        if match.group(2) == new_version:
            continue
        text = text[: match.start()] + match.group(1) + f'"{new_version}"' + text[match.end() :]
        changed = True
    if changed:
        CARGO_LOCK.write_text(text)
    return changed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--check",
        action="store_true",
        help="Exit nonzero if Cargo.toml is out of sync; do not rewrite.",
    )
    args = parser.parse_args()

    py_version = read_python_version()
    cargo_expected = pep440_to_cargo(py_version)
    cargo_actual = read_cargo_version()
    lock_actual = read_cargo_lock_versions()

    if args.check:
        if cargo_actual != cargo_expected:
            print(
                f"Version mismatch: _version.py says {py_version!r} "
                f"(expected Cargo: {cargo_expected!r}), "
                f"Cargo.toml has {cargo_actual!r}. "
                f"Run `python scripts/sync_version.py` to fix.",
                file=sys.stderr,
            )
            return 1
        stale_lock = {c: v for c, v in lock_actual.items() if v != cargo_expected}
        if stale_lock:
            print(
                f"Cargo.lock out of sync: expected {cargo_expected!r} for all "
                f"workspace crates, found {stale_lock!r}. "
                f"Run `python scripts/sync_version.py` to fix.",
                file=sys.stderr,
            )
            return 1
        print(f"Version sync OK: {py_version} ↔ {cargo_actual}")
        return 0

    toml_changed = rewrite_cargo_version(cargo_expected)
    lock_changed = rewrite_cargo_lock(cargo_expected)
    if toml_changed:
        print(
            f"Updated Cargo.toml: [workspace.package].version = "
            f'"{cargo_expected}" (from _version.py={py_version})'
        )
    if lock_changed:
        print(f'Updated Cargo.lock: workspace crate versions → "{cargo_expected}"')
    if not toml_changed and not lock_changed:
        print(f"Cargo.toml and Cargo.lock already in sync: {cargo_actual}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
