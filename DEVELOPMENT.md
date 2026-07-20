# Development

This repo is a Cargo workspace plus a PyO3-built Python package plus a
CMake-installed native library. The whole development loop is driven
by **tox** so that local dev and CI agree on every command.


## Layout

```
qirtoqasm/
├── crates/
│   ├── qirtoqasm-core/              pure-Rust translator (no LLVM, no Python)
│   ├── qirtoqasm-py/                PyO3 bindings → _qirtoqasm_native ext mod
│   └── qirtoqasm-ffi/               C ABI (staticlib + cdylib)
├── python/qirtoqasm/                thin Python re-export shim + canonical _version.py
├── include/qirtoqasm/                C / C++20 headers for the FFI
├── cmake/                          find_package config + C / C++ smoke tests
├── scripts/sync_version.py         propagate _version.py → Cargo.toml
├── test/
│   ├── unit_tests/                 pytest against the Rust-backed wheel
│   └── integ_tests/                fixtures + fixture-parity + e2e (braket / qsharp / cudaq)
├── doc/                            Sphinx docs (RTD-hosted)
└── .github/workflows/              CI
```

Top-level config: `Cargo.toml`, `pyproject.toml`, `tox.ini`,
`CMakeLists.txt`, `rust-toolchain.toml`. `tox.ini` is the single
source of truth for dev + CI; the Rust workspace version is kept in
sync with `python/qirtoqasm/_version.py` by `scripts/sync_version.py`.


## Prerequisites

- Rust stable (MSRV 1.75 is pinned by `rust-toolchain.toml` and enforced
  in CI; contributors can use any newer stable toolchain locally).
- Python ≥ 3.11.
- `maturin` (for building the Python extension):
  `pip install 'maturin>=1.5,<2.0'`
- CMake ≥ 3.21 (only for building / testing the native-library
  install — not needed for Python-only work).
- `cargo-llvm-cov` (for Rust coverage):
  `cargo install cargo-llvm-cov`


## Version management

The **single source of truth** for the package version is
`python/qirtoqasm/_version.py`:

```python
__version__ = "0.1.0.dev0"
```

`scripts/sync_version.py` translates that PEP 440 string to a Cargo
semver form (`0.1.0.dev0` → `0.1.0-dev0`) and rewrites
`Cargo.toml [workspace.package].version`. To bump the version:

```bash
# 1. Edit _version.py.
vim python/qirtoqasm/_version.py

# 2. Propagate to Cargo.toml.
python scripts/sync_version.py

# 3. Commit both files together.
git add python/qirtoqasm/_version.py Cargo.toml
git commit -m "feature: bump version to X.Y.Z"
```

`tox -e linters` and `tox -e linters-check` both run
`sync_version.py --check`, so CI will fail on any change that edits
one file without the other.


## Day-to-day workflow

```bash
# 1. Create a clean Python environment (once).
conda create -n qirtoqasm python=3.11 -y
conda activate qirtoqasm

# 2. Install tox + maturin.
pip install tox 'maturin>=1.5,<2.0'

# 3. Build the Rust-backed wheel editable.
maturin develop --release

# 4. Run the full local suite (matches the default `envlist` in
#    tox.ini).
tox
```

`tox` alone executes: `clean`, `linters`, `docs`, `unit-tests`,
`integ-fixture-parity`, `integ-braket`, `integ-qsharp`, `integ-cudaq`.


## Running individual tiers

| Command                  | What it does |
|--------------------------|--------------|
| `tox -e linters`         | `ruff format` + `ruff check` (rewriting) + version-sync check |
| `tox -e linters-check`   | Same, read-only (CI uses this) |
| `tox -e docs`            | `sphinx-build` → `build/documentation/html` |
| `tox -e serve-docs`      | Serve the built HTML on `http://localhost:8000` |
| `tox -e unit-tests`      | `pytest test/unit_tests/` with coverage, no Braket |
| `tox -e integ-fixture-parity`    | Byte-exact `.ll` → `.qasm` regression + qirrunner vs Braket cross-sim |
| `tox -e integ-braket`    | qirtoqasm → Braket LocalSimulator |
| `tox -e integ-qsharp`    | Q# → QIR → qirtoqasm → Braket (subprocess-isolated) |
| `tox -e integ-cudaq`     | CUDA-Q → QIR → qirtoqasm → Braket (Linux/macOS only) |
| `tox -e integ-tests`     | Run every `integ-*` tier back-to-back |
| `tox -e build-wheel`     | `maturin build` sanity check |
| `tox -e twine-check`     | Build wheel + sdist, run `twine check` |


## Rust workflow

```bash
# Build the whole workspace (all three crates).
cargo build

# Run the Rust test suite (core + ffi).
cargo test -p qirtoqasm-core -p qirtoqasm-ffi

# Lint.
cargo fmt --check
cargo clippy -p qirtoqasm-core -p qirtoqasm-ffi --all-targets -- -D warnings

# Coverage (core only; we don't measure coverage on the PyO3 shim or
# the FFI wrapper because pytest / FFI smoke tests cover them).
cargo llvm-cov --package qirtoqasm-core --summary-only
```

Unit tests live inline inside each `src/*.rs` module as
`#[cfg(test)] mod tests { ... }` blocks. That's the Rust idiom for
white-box coverage of private helpers (`ssa_key`, `paren_depth`, CFG
reduction rule internals, etc.). Integration tests in
`crates/qirtoqasm-core/tests/` (`fixture_parity.rs`, `known_limitations.rs`)
exercise the public API and byte-compare against
`test/integ_tests/fixtures_qir/`.


### Coverage policy

CI enforces **≥ 97% line coverage** on `qirtoqasm-core`. The residual
2–3% is almost entirely:

1. `else { panic!(...) }` diagnostic panics inside `let ... else`
   destructuring in tests (idiomatic Rust for "if the test setup is
   wrong, fail with a readable message").
2. Closing `}` / `continue;` / `break;` tokens that llvm-cov
   instruments as separate regions but doesn't count as reachable
   once their conditional predecessor is taken.
3. Format-string arguments inside uncommon error paths.

We deliberately do **not** sprinkle `// LCOV_EXCL_*` comments to push
the number to 100%. The trade-off: visible annotations everywhere vs.
a small known-gap budget. The gap is explicit in CI; to inspect the
specific uncovered lines locally, run
`cargo llvm-cov -p qirtoqasm-core --lcov --output-path lcov.info` and
open the lcov output in any coverage viewer.


## Python workflow

```bash
# Build and install the Rust-backed wheel into the current venv.
maturin develop --release

# Run the full pytest suite directly (tox also wraps this).
pytest test/unit_tests/ test/integ_tests/ -p no:anyio
```

The `-p no:anyio` flag is important — see "Gotchas" in `AGENTS.md`.

The Python shim at `python/qirtoqasm/__init__.py` is a thin re-export
of `qirtoqasm.translate` / `qirtoqasm.QirToQasmError` / `qirtoqasm.__version__`
from the PyO3 extension. The Rust core does the actual work.


## Native-library workflow

```bash
# Build the release libs.
cargo build --release -p qirtoqasm-ffi
# Produces target/release/libqirtoqasm.a and libqirtoqasm.{so,dylib}.

# CMake install (for consumers that use find_package(qirtoqasm)):
cmake -B build -S . -DCMAKE_INSTALL_PREFIX=/tmp/qirtoqasm-install
cmake --build build
cmake --install build

# Downstream CMake project can then:
#   find_package(qirtoqasm REQUIRED)
#   target_link_libraries(my_target PRIVATE qirtoqasm::qirtoqasm)
#   target_compile_features(my_target PRIVATE cxx_std_20)  # C++ consumers

# Optional: build the C++20 smoke test.
cmake -B build -S . -DQIRTOQASM_BUILD_CPP_TESTS=ON
cmake --build build --target qirtoqasm_cpp_smoke
./build/qirtoqasm_cpp_smoke

# Optional: build the pure-C smoke test.
cmake -B build -S . -DQIRTOQASM_BUILD_C_TESTS=ON
cmake --build build --target qirtoqasm_c_smoke
./build/qirtoqasm_c_smoke
```


## CI overview

All CI lives in `.github/workflows/`:

- **`ci.yml`** — the main pipeline. Runs on pushes to `main` and on
  pull requests. Jobs: `build` (3 OS × 3 py, cargo build/test + tox
  unit-tests + Codecov), `msrv` (rust 1.75 on Ubuntu), `coverage`
  (cargo-llvm-cov ≥ 97%), `integ-fixture-parity`, `integ-braket`,
  `integ-qsharp`, `integ-cudaq` (Linux + macOS only), `cpp-smoke`
  and `c-smoke` (each on Ubuntu + macOS + Windows, built via CMake
  with warnings-as-errors, asserting on translation output for all
  four API call shapes — default options, populated options, error
  path, version). Integ jobs `needs: [build, cpp-smoke, c-smoke]`
  so a broken native ABI blocks them.
- **`wheels.yml`** — `cibuildwheel` per-platform wheel matrix. A
  smoke build runs on PRs (Ubuntu only); the full matrix runs on
  release publication and on `v*` tag pushes. Linux x86_64 +
  aarch64 (manylinux2014 + musllinux_1_2), macOS x86_64 + arm64,
  Windows x86_64. One `cp311-abi3` wheel per platform — the abi3
  tag means that wheel is also installable on 3.12 and 3.13.
- **`publish-to-pypi.yml`** — uploads the `wheels.yml` artifacts to
  PyPI via trusted publishing (OIDC). Triggered on release
  publication.
- **`twine-check.yml`** — builds an sdist and runs `twine check` on
  every PR.
- **`check-format.yml`** — runs `tox -e linters-check` on every PR.
- **`code-freeze.yml`** — gates PR merges during release freeze
  windows (uses `vars.FROZEN` / `vars.UNFROZEN_PREFIX`).
- **`dependabot.yml`** — weekly GitHub Actions + Cargo (grouped
  minor/patch) + pip dependency updates.


## Releasing

Releases are automated. Merges to `main` are grouped by their
conventional-commit prefix (see `CONTRIBUTING.md`) and cut into
tagged releases without manual intervention — follow the commit
prefix conventions and the rest happens automatically.

The tag triggers `wheels.yml` (per-platform wheel + sdist build)
and `publish-to-pypi.yml` (upload to PyPI via the OIDC trusted
publisher configured in the `pypi` GitHub Actions environment).

You do not manually edit `_version.py`, tag anything, or draft
a GitHub release yourself.
