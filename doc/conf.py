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

"""Sphinx configuration for the qirtoqasm documentation site.

Hosted on Read the Docs. See ``.readthedocs.yml`` at the repo root.
"""

from __future__ import annotations

import os
import sys
from datetime import datetime

sys.path.insert(0, os.path.abspath("../python"))

# -- Project information -----------------------------------------------------

project = "qirtoqasm"
author = "Amazon Web Services"
copyright = f"{datetime.now().year}, {author}"  # noqa: A001

# Read the version from the single source of truth.
_version_globals: dict = {}
with open(
    os.path.join(os.path.dirname(__file__), "..", "python", "qirtoqasm", "_version.py"),
    encoding="utf-8",
) as _vf:
    exec(_vf.read(), _version_globals)  # noqa: S102
release = _version_globals["__version__"]
version = ".".join(release.split(".")[:2])

# -- General configuration ---------------------------------------------------

extensions = [
    "sphinx.ext.autodoc",
    "sphinx.ext.napoleon",
    "sphinx.ext.intersphinx",
    "sphinx.ext.viewcode",
    "sphinxcontrib.apidoc",
]

templates_path = ["_templates"]
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]

# -- autodoc / apidoc --------------------------------------------------------

# Mock the native extension so autodoc can import the shim without a
# Rust toolchain on the docs runner.
autodoc_mock_imports = ["qirtoqasm._qirtoqasm_native"]

autodoc_default_options = {
    "members": True,
    "undoc-members": True,
    "show-inheritance": True,
}

apidoc_module_dir = "../python/qirtoqasm"
apidoc_output_dir = "_apidoc"
apidoc_excluded_paths = ["_qirtoqasm_native*"]
apidoc_separate_modules = True
apidoc_module_first = True
apidoc_toc_file = False

# -- Intersphinx -------------------------------------------------------------

intersphinx_mapping = {
    "python": ("https://docs.python.org/3", None),
}

# -- HTML output -------------------------------------------------------------

html_theme = "sphinx_rtd_theme"
html_static_path: list[str] = []
