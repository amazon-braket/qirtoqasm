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

"""Sync the packaging versions in ``pyproject.toml`` and ``Cargo.toml``
with the Python ``_version.py`` source of truth.

``_version.py`` is authoritative because Braket CI expects
``<package>/_version.py`` to be the single place versions live.
``pyproject.toml``'s ``[project].version`` gets the PEP 440 string
verbatim — maturin publishes it as the distribution version, so it must
be a static key. ``Cargo.toml``'s ``[workspace.package].version`` gets
the semver translation.

Usage:

    # Rewrite pyproject.toml/Cargo.toml to match _version.py.
    python scripts/sync_version.py

    # Fail if they're out of sync without rewriting (CI gate).
    python scripts/sync_version.py --check
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
VERSION_PY = REPO_ROOT / "python" / "qirtoqasm" / "_version.py"
PYPROJECT_TOML = REPO_ROOT / "pyproject.toml"
CARGO_TOML = REPO_ROOT / "Cargo.toml"
CARGO_LOCK = REPO_ROOT / "Cargo.lock"

_WORKSPACE_CRATES = ("qirtoqasm-core", "qirtoqasm-ffi", "qirtoqasm-py")
_VERSION_PY_RE = re.compile(r'^__version__\s*=\s*"([^"]+)"\s*$', re.MULTILINE)


def _section_version_re(section: str) -> re.Pattern[str]:
    """Match the ``version = "..."`` key inside a given TOML section.

    The ``(?!^\\[)`` lookahead stops the scan at the next section header,
    so ``[`` inside comments and inline arrays is harmless.
    """
    return re.compile(
        rf'(?m)^(\[{re.escape(section)}\]\n(?:(?!^\[)[\s\S])*?^version\s*=\s*)"([^"]+)"'
    )


_PYPROJECT_VERSION_RE = _section_version_re("project")
_CARGO_VERSION_RE = _section_version_re("workspace.package")


def read_python_version() -> str:
    text = VERSION_PY.read_text()
    match = _VERSION_PY_RE.search(text)
    if not match:
        raise SystemExit(f"Could not find __version__ in {VERSION_PY}")
    return match.group(1)


def _read_toml_version(path: Path, pattern: re.Pattern[str], what: str) -> str:
    match = pattern.search(path.read_text())
    if not match:
        raise SystemExit(f"Could not find {what} in {path}")
    return match.group(2)


def _rewrite_toml_version(
    path: Path, pattern: re.Pattern[str], what: str, new_version: str
) -> bool:
    """Rewrite ``path``'s version to ``new_version``. Return True if changed."""
    text = path.read_text()
    match = pattern.search(text)
    if not match:
        raise SystemExit(f"Could not find {what} in {path}")
    if match.group(2) == new_version:
        return False
    new_text = text[: match.start()] + match.group(1) + f'"{new_version}"' + text[match.end() :]
    path.write_text(new_text)
    return True


def read_pyproject_version() -> str:
    return _read_toml_version(PYPROJECT_TOML, _PYPROJECT_VERSION_RE, "[project].version")


def read_cargo_version() -> str:
    return _read_toml_version(CARGO_TOML, _CARGO_VERSION_RE, "[workspace.package].version")


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


def rewrite_pyproject_version(new_version: str) -> bool:
    """Rewrite ``pyproject.toml`` to ``new_version``. Return True if changed."""
    return _rewrite_toml_version(
        PYPROJECT_TOML, _PYPROJECT_VERSION_RE, "[project].version", new_version
    )


def rewrite_cargo_version(new_version: str) -> bool:
    """Rewrite ``Cargo.toml`` to ``new_version``. Return True if changed."""
    return _rewrite_toml_version(
        CARGO_TOML, _CARGO_VERSION_RE, "[workspace.package].version", new_version
    )


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
        help="Exit nonzero if pyproject.toml or Cargo.toml is out of sync; do not rewrite.",
    )
    args = parser.parse_args()

    py_version = read_python_version()
    cargo_expected = pep440_to_cargo(py_version)
    pyproject_actual = read_pyproject_version()
    cargo_actual = read_cargo_version()
    lock_actual = read_cargo_lock_versions()

    if args.check:
        if pyproject_actual != py_version:
            print(
                f"Version mismatch: _version.py says {py_version!r}, "
                f"pyproject.toml [project].version has {pyproject_actual!r}. "
                f"Run `python scripts/sync_version.py` to fix.",
                file=sys.stderr,
            )
            return 1
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

    pyproject_changed = rewrite_pyproject_version(py_version)
    toml_changed = rewrite_cargo_version(cargo_expected)
    lock_changed = rewrite_cargo_lock(cargo_expected)
    if pyproject_changed:
        print(f'Updated pyproject.toml: [project].version = "{py_version}"')
    if toml_changed:
        print(
            f"Updated Cargo.toml: [workspace.package].version = "
            f'"{cargo_expected}" (from _version.py={py_version})'
        )
    if lock_changed:
        print(f'Updated Cargo.lock: workspace crate versions → "{cargo_expected}"')
    if not pyproject_changed and not toml_changed and not lock_changed:
        print(f"pyproject.toml, Cargo.toml and Cargo.lock already in sync: {py_version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
