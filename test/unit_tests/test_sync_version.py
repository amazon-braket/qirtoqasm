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

"""Regression tests for ``scripts/sync_version.py``.

The published version must never carry a PEP 440 *local* segment (the
part after a ``+``); PyPI rejects those with a 400. That is not
hypothetical — release ``v0.1.0.post0`` failed to upload because the
wheel version was sourced from Cargo, where ``.postN`` can only be
encoded as build metadata (``0.1.0+post0``).

These tests pin the two halves of the arrangement that prevents it:
``pyproject.toml`` carries the PEP 440 string statically, and Cargo
keeps its own semver spelling.
"""

from __future__ import annotations

import importlib.util
import shutil
import sys
from pathlib import Path

import pytest
from packaging.version import Version

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "sync_version.py"


def _load_sync_version():
    """Import ``scripts/sync_version.py``, which is not an installed module."""
    spec = importlib.util.spec_from_file_location("sync_version", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


sync_version = _load_sync_version()

# Every version shape the internal release pipeline can emit. It derives
# the increment from commit prefixes (major / minor / patch / post) and
# hardcodes `dev` for the post-release development bump.
PIPELINE_VERSIONS = [
    ("1.2.3", "1.2.3"),
    ("1.2.3.dev0", "1.2.3-dev0"),
    ("1.2.3.post0", "1.2.3+post0"),
    ("1.2.3.post7", "1.2.3+post7"),
]


@pytest.mark.parametrize(("pep440", "cargo"), PIPELINE_VERSIONS)
def test_pep440_to_cargo(pep440, cargo):
    assert sync_version.pep440_to_cargo(pep440) == cargo


@pytest.mark.parametrize(("pep440", "cargo"), PIPELINE_VERSIONS)
def test_published_version_has_no_local_segment(pep440, cargo):
    """The Cargo spelling may use ``+``; the published one must not."""
    assert Version(pep440).local is None
    if "+" in cargo:
        assert Version(cargo).local is not None, "expected the Cargo form to be PyPI-illegal"


def test_pyproject_version_is_static():
    """``[project].version`` must not be dynamic.

    As a dynamic field maturin sources the version from Cargo, which is
    exactly how the local-version segment reached PyPI.
    """
    pyproject = (REPO_ROOT / "pyproject.toml").read_text()
    assert 'dynamic = ["version"]' not in pyproject
    assert sync_version.read_pyproject_version()


def test_repo_is_in_sync():
    """Same invariant the ``version-sync`` tox env gates on."""
    declared = sync_version.read_python_version()
    assert sync_version.read_pyproject_version() == declared
    assert sync_version.read_cargo_version() == sync_version.pep440_to_cargo(declared)


def test_sync_propagates_a_post_release(tmp_path, monkeypatch):
    """End-to-end through ``main()``: the entry point the release pipeline runs."""
    for name in ("pyproject.toml", "Cargo.toml", "Cargo.lock"):
        shutil.copy(REPO_ROOT / name, tmp_path / name)
    version_py = tmp_path / "_version.py"
    version_py.write_text('__version__ = "0.1.0.post0"\n')

    monkeypatch.setattr(sync_version, "VERSION_PY", version_py)
    monkeypatch.setattr(sync_version, "PYPROJECT_TOML", tmp_path / "pyproject.toml")
    monkeypatch.setattr(sync_version, "CARGO_TOML", tmp_path / "Cargo.toml")
    monkeypatch.setattr(sync_version, "CARGO_LOCK", tmp_path / "Cargo.lock")
    monkeypatch.setattr(sys, "argv", ["sync_version.py"])

    assert sync_version.main() == 0

    # Published version: PEP 440 verbatim, no local segment.
    assert sync_version.read_pyproject_version() == "0.1.0.post0"
    assert Version(sync_version.read_pyproject_version()).local is None
    # Cargo: semver build metadata, never published.
    assert sync_version.read_cargo_version() == "0.1.0+post0"
    assert set(sync_version.read_cargo_lock_versions().values()) == {"0.1.0+post0"}

    # And the result is self-consistent, so the CI gate passes on it.
    monkeypatch.setattr(sys, "argv", ["sync_version.py", "--check"])
    assert sync_version.main() == 0


def test_check_fails_when_pyproject_drifts(tmp_path, monkeypatch):
    """A stale pyproject.toml must fail the gate rather than publish the wrong version."""
    for name in ("pyproject.toml", "Cargo.toml", "Cargo.lock"):
        shutil.copy(REPO_ROOT / name, tmp_path / name)
    version_py = tmp_path / "_version.py"
    version_py.write_text('__version__ = "9.9.9"\n')

    monkeypatch.setattr(sync_version, "VERSION_PY", version_py)
    monkeypatch.setattr(sync_version, "PYPROJECT_TOML", tmp_path / "pyproject.toml")
    monkeypatch.setattr(sync_version, "CARGO_TOML", tmp_path / "Cargo.toml")
    monkeypatch.setattr(sync_version, "CARGO_LOCK", tmp_path / "Cargo.lock")
    monkeypatch.setattr(sys, "argv", ["sync_version.py", "--check"])

    assert sync_version.main() == 1
