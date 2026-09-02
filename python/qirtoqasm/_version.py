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

"""Single source of truth for the qirtoqasm package version.

This file is the authoritative version declaration for the entire
repository:

- The Python wheel version (``qirtoqasm.__version__``) reads from here.
- The Rust workspace version in ``Cargo.toml`` is kept in sync by
  ``scripts/sync_version.py`` which ``maturin`` invokes before building
  the wheel (via ``tool.maturin.python-packages`` metadata hooks), and
  which ``tox -e linters`` enforces as a CI-gated check.

To bump the version, edit ONLY the ``__version__`` string below and run
``python scripts/sync_version.py``. CI will fail if the two are out of
sync. Do not edit ``Cargo.toml``'s ``[workspace.package].version``
directly.
"""

__version__ = "0.1.0.post0"
