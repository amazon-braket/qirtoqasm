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
from pathlib import Path

from sphinx.application import Sphinx

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
exclude_patterns = ["_build", "_apidoc", "Thumbs.db", ".DS_Store"]

# -- autodoc / apidoc --------------------------------------------------------

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

LLMS_TXT_TITLE = "qirtoqasm"
LLMS_TXT_SUMMARY = (
    "Translate QIR (Quantum Intermediate Representation) programs to "
    "Braket-compatible OpenQASM 3.0."
)
LLMS_TXT_BASE_URL = "https://qirtoqasm.readthedocs.io/en/stable/"
LLMS_TXT_SECTIONS: dict[str, tuple[str, ...]] = {
    "Docs": (),
    # The generated _apidoc tree is in exclude_patterns, so api.rst is the API page.
    "API Reference": ("api",),
}


def _llms_txt_section(docname: str) -> str:
    """Return the llms.txt section heading a document belongs under."""
    for heading, prefixes in LLMS_TXT_SECTIONS.items():
        if any(docname.startswith(prefix) for prefix in prefixes):
            return heading
    default_heading, _ = next(iter(LLMS_TXT_SECTIONS.items()))
    return default_heading


def _write_llms_txt(app: Sphinx, exception: Exception | None) -> None:
    """Write llms.txt, a manifest of every built page for LLM discoverability.

    The format follows https://llmstxt.org: an H1 name, a blockquote summary, then
    one file list per H2 section. Pages are grouped so that an agent can tell
    narrative docs and generated API reference apart.
    """
    if exception or app.builder.name != "html":
        return

    # Read the Docs passes the canonical URL to every build automatically, so this
    # is set in any RTD build and the default only applies elsewhere. See
    # https://docs.readthedocs.com/platform/stable/canonical-urls.html#how-to-specify-the-canonical-url
    base_url = os.environ.get("READTHEDOCS_CANONICAL_URL", LLMS_TXT_BASE_URL)
    if base_url and not base_url.endswith("/"):
        base_url += "/"

    env = app.env
    sections: dict[str, list[str]] = {heading: [] for heading in LLMS_TXT_SECTIONS}
    for docname in sorted(env.all_docs):
        url = f"{base_url}{app.builder.get_target_uri(docname)}"
        sections[_llms_txt_section(docname)].append(f"- [{env.titles[docname].astext()}]({url})")

    lines = [f"# {LLMS_TXT_TITLE}", "", f"> {LLMS_TXT_SUMMARY}"]
    for heading in LLMS_TXT_SECTIONS:
        if sections[heading]:
            lines += ["", f"## {heading}", "", *sections[heading]]

    out = Path(app.outdir) / "llms.txt"
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"--> Wrote {out.name}")


def setup(app: Sphinx) -> None:
    """Register build hooks."""
    app.connect("build-finished", _write_llms_txt)
