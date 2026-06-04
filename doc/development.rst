Development
===========

For the full developer workflow, including Rust-specific details, see
``DEVELOPMENT.md`` at the repo root.

This page is a quick reference for the tasks most frequently performed
from the command line.


Clone and build
---------------

.. code-block:: bash

   git clone https://github.com/amazon-braket/qirtoqasm.git
   cd qirtoqasm

   # Create a clean Python 3.11 environment.
   conda create -n qirtoqasm python=3.11 -y
   conda activate qirtoqasm

   # Build the Rust-backed wheel and install it editable.
   pip install 'maturin>=1.5,<2.0'
   maturin develop --release


Running the test suites
-----------------------

The project uses tox for every developer workflow. Running ``tox`` by
itself executes the full pre-PR suite (lint, docs, unit tests, golden
regression tests, and the Braket + Q# end-to-end tests).

.. code-block:: bash

   # Lint + format + version-sync check.
   tox -e linters

   # Unit tests (no Braket / Q# / CUDA-Q dependencies).
   tox -e unit-tests

   # Golden .ll / .qasm regression suite (no Braket).
   tox -e integ-golden

   # qirtoqasm → Braket LocalSimulator end-to-end.
   tox -e integ-braket

   # Q# → qirtoqasm → Braket (subprocess-isolated).
   tox -e integ-qsharp

   # CUDA-Q → qirtoqasm → Braket (subprocess-isolated;
   # Linux and macOS only — no Windows wheels for cudaq).
   tox -e integ-cudaq

   # Build this documentation site.
   tox -e docs


Releasing
---------

1. Edit ``python/qirtoqasm/_version.py`` and bump ``__version__``.
2. Run ``python scripts/sync_version.py`` to propagate the new version
   to ``Cargo.toml``.
3. Run ``tox`` locally to verify the full suite passes.
4. Commit the version bump and open a pull request.
5. After merge, draft and publish a GitHub release. The
   ``publish-to-pypi.yml`` workflow picks up the release, downloads
   the wheel and sdist artifacts produced by ``wheels.yml``, and
   uploads them via PyPI trusted publishing.
