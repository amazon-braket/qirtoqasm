# AGENTS.md — orientation for AI coding agents and new contributors

Read this first. It's the single entry point that tells you what the
project is, where to find each piece, how to build and test, which
invariants must be preserved, and which commands to run for every
routine task.


## Table of contents

- [What this repo is](#what-this-repo-is)
- [Repository layout](#repository-layout)
- [Quick-start commands](#quick-start-commands)
- [Build, test, lint cheat sheet](#build-test-lint-cheat-sheet)
- [Version management](#version-management)
- [Invariants — things that must stay true](#invariants--things-that-must-stay-true)
- [Style rules for prose in code, comments, and docs](#style-rules-for-prose-in-code-comments-and-docs)
- [Scope: what qirtoqasm does and doesn't lower](#scope-what-qirtoqasm-does-and-doesnt-lower)
- [Module map — where to go for each subsystem](#module-map--where-to-go-for-each-subsystem)
- [Test layers](#test-layers)
- [CI layout](#ci-layout)
- [Release process](#release-process)
- [Gotchas](#gotchas)


## What this repo is

`qirtoqasm` translates **QIR** (Quantum Intermediate Representation — a
subset of LLVM IR with quantum-specific intrinsics) to
**Braket-compatible OpenQASM 3.0** text. It supports the QIR Base
Profile (static circuits) and the Adaptive Profile (mid-circuit
measurement with classical branching via
``__quantum__qis__read_result__body`` + ``br i1``).

The project is a Cargo workspace with three crates and a Python
façade:

- `qirtoqasm-core` — pure-Rust translator. No LLVM, no llvmlite, no
  Python. The real work lives here.
- `qirtoqasm-py` — PyO3 bindings that expose the core to Python as
  the `_qirtoqasm_native` extension module (PyO3 `abi3-py311`).
- `qirtoqasm-ffi` — `libqirtoqasm.{a,dylib,so}` C ABI + a C/C++ header
  + a CMake package config, for C/C++ consumers.

The Python package `qirtoqasm` is a thin shim over `_qirtoqasm_native`
and exposes a single public function,
``qirtoqasm.translate(qir_text, *, producer=None) -> str``. Every
tunable is a keyword-only kwarg; the Rust core's
``translate(qir_text, &TranslateOptions)`` exposes the same surface,
and the C ABI uses a versioned ``qirtoqasm_options_t`` struct (see
"Public API: the options pattern" below). Errors surface as
``qirtoqasm.QirToQasmError``.

### Braket is the target

The whole point is that the emitter's output can be handed directly
to `braket.ir.openqasm.Program` with no further rewriting. Gate-name
choices flow from this:

- Two-qubit Ising rotations emit as `xx` / `yy` / `zz`, not the
  `stdgates.inc` names `rxx` / `ryy` / `rzz`.
- CNOT / Toffoli emit as `cnot` / `ccnot`, not `cx` / `ccx`.
- Classical registers are declared as plain `bit[N] c;`, without the
  OpenQASM `output` qualifier.
- `include "stdgates.inc";` is never emitted.

These choices are not configurable on the public API — qirtoqasm's job
is to emit Braket-compatible OpenQASM 3. Callers targeting a different
OpenQASM consumer post-process the output (gate-name substitutions,
stdgates.inc injection) rather than driving a profile knob through
`translate`.


## Public API: the options pattern

All four public faces (Python, Rust crate, C ABI, C++20 header)
expose a single one-stage ``translate`` whose per-call tunables flow
through an **options object** rather than positional parameters.
That shape is what keeps new fields additive: no consumer's code
changes when we add a new tunable.

| Face   | Entry point                                            | Options carrier                                       |
|--------|--------------------------------------------------------|-------------------------------------------------------|
| Python | `translate(qir_text, *, producer=None)`                | keyword-only kwargs                                   |
| Rust   | `translate(qir_text: &str, &TranslateOptions)`         | `#[non_exhaustive]` struct + builder setters          |
| C      | `qirtoqasm_translate(qir, const qirtoqasm_options_t*, …)`| versioned struct with `struct_version` / `struct_size`|
| C++    | `qirtoqasm::translate(qir, const Options& = {})`        | plain struct, C++20 designated initializers           |

### Why each face looks the way it does

- **Python**: PEP 3102 keyword-only args (`*, producer=None`) are
  already the idiomatic append-only signature shape. No options
  struct needed.
- **Rust**: `TranslateOptions` is marked `#[non_exhaustive]` so
  downstream crates cannot construct it with positional struct
  literals and cannot exhaustive-match it. Adding a field is
  therefore non-breaking. External callers use
  `TranslateOptions::default().with_producer(...)` or equivalent
  builder methods; `..Default::default()` struct-update syntax is
  reserved for intra-crate use where `#[non_exhaustive]` does not
  restrict construction.
- **C**: the versioned-struct pattern (LLVM, CUDA Driver, Vulkan,
  sqlite3_vfs all use variants of this). Callers call
  `qirtoqasm_options_init` first, which sets
  `struct_version = QIRTOQASM_OPTIONS_VERSION` and
  `struct_size = sizeof(qirtoqasm_options_t)`, then patch fields. The
  library validates both before reading, and will size-gate future
  fields against the caller's declared size so a binary built
  against an older header still works. `NULL` options means
  "defaults for everything" — the common case stays a one-liner.
- **C++**: a plain `Options` struct with defaulted fields, a single
  `translate(qir, const Options& = {})` overload, and C++20
  designated initializers for field-by-name construction:
  `qirtoqasm::translate(qir, { .producer = "mylib 0.1.2" })`. The
  inline wrapper forwards to the C ABI so there is only one place
  where options semantics live.

### Alternatives considered and rejected

1. **Positional parameters per option** (e.g. adding a second
   positional arg `translate(qir, producer)`): breaks every caller
   on every new field. Rejected up front.
2. **Versioned entry points** (`translate_v2`, `translate_v3`, ...):
   would work at the ABI level but bloats the surface, kills
   Python ergonomics, and makes mix-and-match of options impossible.
3. **Free-form JSON / key=value config string**: maximally
   forward-compatible but loses type safety, IDE completion, and
   just moves the schema-versioning problem into a string parser
   we'd have to own.
4. **Opaque handle + setter functions** for the C ABI
   (`qirtoqasm_options_new`, `..._set_producer`, `..._free`): works
   and is ABI-clean, but adds allocation and lifetime management to
   every C caller, and a three-line happy path for something that
   could be a one-liner. The versioned struct gives us append-only
   additivity without the runtime cost.

### Invariants on the options pattern

- `TranslateOptions` must stay `#[non_exhaustive]`. Don't remove it
  even if it would simplify local code; the attribute is what makes
  append-only non-breaking.
- New fields on `qirtoqasm_options_t` are **appended** after the
  current tail. Never reorder, never remove, never shrink a field.
  Bump `QIRTOQASM_OPTIONS_VERSION` whenever an added field's
  default-zero interpretation differs from the library's desired
  default for older callers.
- New fields appear on **all four faces**. If adding a field makes
  sense for Python only, step back — it probably belongs in the
  caller, not on `translate`.
- JSON keys emitted in the trailing `// generated-by:` comment are
  part of the observable surface and are byte-stable in fixture-parity
  fixtures.


## Repository layout

```
qirtoqasm/
├── crates/
│   ├── qirtoqasm-core/              pure-Rust translator
│   ├── qirtoqasm-py/                PyO3 bindings → _qirtoqasm_native ext mod
│   └── qirtoqasm-ffi/               C ABI (staticlib + cdylib)
├── python/qirtoqasm/                thin re-export shim + canonical _version.py
├── include/qirtoqasm/               C / C++ headers for the FFI
├── cmake/                          find_package config + smoke tests
├── scripts/sync_version.py         propagate _version.py → Cargo.toml
├── test/
│   ├── unit_tests/                 pytest against the wheel (no Braket)
│   └── integ_tests/                fixtures + fixture-parity + e2e (braket / qsharp / cudaq)
├── doc/                            Sphinx docs (Read the Docs)
└── .github/workflows/              CI
```

Top-level config files (`Cargo.toml`, `pyproject.toml`, `tox.ini`,
`CMakeLists.txt`, `MANIFEST.in`, `rust-toolchain.toml`) live at the
root. `tox.ini` is the single source of truth for dev + CI commands;
`scripts/sync_version.py` keeps `Cargo.toml`'s workspace version in
lock-step with `python/qirtoqasm/_version.py`.


## Quick-start commands

First time on a fresh machine:

```bash
# 1. Create a clean Python env (conda or venv). Python ≥ 3.11.
conda create -n qirtoqasm python=3.11 -y
conda activate qirtoqasm

# 2. Install tox + maturin.
pip install tox 'maturin>=1.5,<2.0'

# 3. Build the Rust-backed wheel editable.
maturin develop --release

# 4. Run the full pre-PR suite.
tox
```

The default `tox` envlist runs: `clean,linters,docs,unit-tests,
integ-fixture-parity,integ-braket,integ-qsharp,integ-cudaq`.


## Build, test, lint cheat sheet

Every developer workflow goes through tox. CI invokes the same envs,
so local success means CI success.

| Command                  | What it does |
|--------------------------|--------------|
| `tox`                    | Full pre-PR suite (see above) |
| `tox -e linters`         | Rewrite formatting, run ruff check + version-sync |
| `tox -e linters-check`   | Read-only lint (CI uses this) |
| `tox -e docs`            | Build Sphinx docs → `build/documentation/html/` |
| `tox -e serve-docs`      | Serve the built HTML on `http://localhost:8000` |
| `tox -e unit-tests`      | pytest unit suite (no Braket); 100% coverage required |
| `tox -e integ-fixture-parity`    | `.ll` → `.qasm` byte-exact regression + qirrunner vs Braket cross-sim |
| `tox -e integ-braket`    | qirtoqasm → Braket LocalSimulator |
| `tox -e integ-qsharp`    | Q# → QIR → qirtoqasm → Braket (subprocess-isolated) |
| `tox -e integ-cudaq`     | CUDA-Q → QIR → qirtoqasm → Braket (Linux + macOS) |
| `tox -e integ-tests`     | Run every `integ-*` tier back-to-back |
| `tox -e build-wheel`     | `maturin build` smoke (CI uses cibuildwheel) |
| `tox -e twine-check`     | Build wheel + sdist, run `twine check` |

Pass pytest args through tox with `--`:
`tox -e unit-tests -- -k test_bell -v`.

Direct Rust commands when iterating on the core:

```bash
# Full workspace build (default members — excludes qirtoqasm-py because
# its pyo3 cdylib needs maturin link flags).
cargo build

cargo test -p qirtoqasm-core -p qirtoqasm-ffi

cargo fmt --all -- --check
cargo clippy -p qirtoqasm-core -p qirtoqasm-ffi --all-targets -- -D warnings

# Coverage (floor 97% on qirtoqasm-core, enforced in CI).
cargo llvm-cov --package qirtoqasm-core --summary-only
```


## Version management

**`python/qirtoqasm/_version.py` is the single source of truth.**
`scripts/sync_version.py` translates the PEP 440 string to a Cargo
semver form and rewrites `Cargo.toml [workspace.package].version`.
Both `tox -e linters` and `tox -e linters-check` run the script with
`--check`, so drift cannot land.

To bump the version:

```bash
# 1. Edit _version.py.
vim python/qirtoqasm/_version.py

# 2. Propagate.
python scripts/sync_version.py

# 3. Commit both files together.
git add python/qirtoqasm/_version.py Cargo.toml
```


## Invariants — things that must stay true

These gates are enforced in CI. Break them and the build will fail.

1. **`python/qirtoqasm/_version.py` and `Cargo.toml` agree.** Run
   `python scripts/sync_version.py` after editing `_version.py`.
2. **Python shim stays thin.** `python/qirtoqasm/__init__.py` is
   re-exports only. New behavior belongs in the Rust core.
3. **Unit-test coverage is 100%** on the Python shim. Extend tests
   when you extend the shim.
4. **Rust core coverage is ≥ 97%.** cargo-llvm-cov floor in
   `.github/workflows/ci.yml` → `coverage` job.
5. **Fixture-parity checks are byte-exact.** Changing emitter output that
   shifts any fixture is a deliberate regression; update the printer,
   not the fixture.
6. **MSRV is Rust 1.75.** Declared in `Cargo.toml`
   `[workspace.package].rust-version` and enforced by the `msrv` CI
   job.
7. **No Braket / Q# / CUDA-Q imports in the unit tier.** The build
   step of CI runs in a minimal environment; any import there breaks
   it.
8. **Braket is the OpenQASM target.** The emitter uses Braket-native
   gate names (`cnot`, `ccnot`, `xx` / `yy` / `zz`) and plain
   `bit[N] c;` declarations. Changes here are profile-level decisions.
9. **No LLVM or llvmlite dependency** at build or runtime. The Rust
   core parses QIR text directly.


## Style rules for prose in code, comments, and docs

These aren't enforced mechanically, but they set the tone for what
lands in the repo. Reviewers apply them by hand.

- **American English throughout.** Prefer `recognize` over
  `recognise`, `behavior` over `behaviour`, `normalize` over
  `normalise`, `modeled` over `modelled`, `optimize` over
  `optimise`, and so on. This applies to doc comments, module
  headers, error messages, test names, and prose in Markdown /
  Sphinx files. Real symbol names that happen to embed a
  British-spelling substring (e.g. a QIR intrinsic whose canonical
  name literally embeds `initialised`) are the actual identifier
  and stay as-is.

- **Describe LLVM patterns, not who emitted them.** Doc comments
  and per-gate translation notes should describe the shape being
  matched (e.g. "canonical form of `i8*` after
  `signatures::canonicalize_type`") rather than naming a specific
  producer or vendor (Q#, CUDA-Q, Quantinuum, IonQ, Rigetti,
  Microsoft, NVIDIA, etc.) as the source of the pattern. Naming
  producers is fine in the integ-runner files that genuinely
  invoke them, in package metadata that lists them as optional
  extras, in CI configuration that documents the test matrix, and
  in user-facing docs (`README.md`, `doc/*.rst`, `AGENTS.md`,
  `DEVELOPMENT.md`, `CONTRIBUTING.md`) where naming a producer is
  part of the user-help surface.

- **Producer examples use a generic placeholder.** The
  `producer=` keyword on `translate()` accepts an arbitrary label
  that surfaces in the trailing `// generated-by:` comment;
  examples in docs and doc comments should use a generic value
  like `"mylib 0.1.2"` rather than naming a specific real tool.
  The integ-tier tests are the exception — they legitimately
  drive `translate(...)` with the real tool label the fixture
  came from.

- **Describe the code as it is, not as it was.** Comments and
  docstrings should describe present-tense behavior. Avoid
  narrative like `Previously this did X` / `This used to
  panic` / `Regression test for the <foo> fix` — a reader coming
  to the file cold has no context for what changed. If a
  comment's value is proportional to knowing the pre-fix
  version, delete it.

- **No cross-references to a phantom Python implementation.**
  There is no Python implementation of qirtoqasm; the whole
  translator is Rust. Rustdoc comments should describe what a
  type/function does on its own terms rather than saying things
  like "Mirrors `src/qirtoqasm/foo.py`" or "the Rust equivalent
  of Python's <X> class".

- **Test names must match what the assertions verify.** A test
  named `test_X_names_Y` must assert that `Y` appears in the
  observable output/error, not just that some related generic
  phrase does. Test docstrings must not overpromise: if the
  docstring says "verifies both A and B" the assertions must
  check both, or the docstring should be scoped down.

- **Integ tests: assert on emitted OpenQASM when the classical
  distribution alone can't distinguish the test's stated
  intent.** A test named `test_mcm_if_else_on_braket` whose only
  check is a uniform 50/50 distribution over `{00, 11}` will
  pass even if the else branch was silently dropped (`z` on |0⟩
  is a no-op, giving the same distribution as `mcm_if.ll`). Add
  a structural `assert "..." in openqasm` that pins the specific
  OpenQASM shape the test name promises, alongside the
  distribution check.

- **Python test files: module-level state at the top.** Imports,
  module-level constants, and `pytest.importorskip(...)` calls
  belong above the first `def`, not interleaved between function
  bodies. Env-controlled extras are guaranteed at the env level;
  defensive lazy imports mid-file are over-cautious.

- **No test classes, no banner-comment section dividers.** Flat
  `def test_*()` at module level; `pytest` doesn't require class
  organization, and banner comments like `# --- Section ---`
  become drift-prone as tests get added or reordered.


## Scope: what qirtoqasm does and doesn't lower

### Supported

- QIS gates with the `__quantum__qis__<name>__body` naming convention:
  h, x, y, z, s, t, cnot/cx, cy, cz, swap, rx, ry, rz, ccx/ccnot,
  rxx, ryy, rzz, reset, mz/m/mresetz, phasedx, and the `__adj`
  adjoints for non-self-adjoint gates.
- Mid-circuit measurement via `__quantum__qis__read_result__body` and
  the alias `__quantum__rt__read_result`, with `br i1` branches.
- **Compound Boolean conditions**: all ten LLVM integer `icmp`
  predicates, bitwise i1 `and`/`or`/`xor` (including the
  `xor i1 %x, true` logical-NOT idiom), `select i1`, and `phi i1`
  short-circuit merges.
- **Integer arithmetic** on classical SSA values (`add`, `sub`, `mul`
  with `nuw`/`nsw`/`exact` flags). Inlined at the use site.
- **`phi i32` / `phi i64` integer accumulation**: two-incoming
  if-merge phis lower to an OpenQASM 3 `int cint_N = <init>;` plus
  conditional `cint_N = <update>;` assignments. Chains of phis share
  the same `cint_N` variable.
- **`select i1 %cond, iN A, iN B` integer cascade**: lowers to
  `(cond) * A + (1 - cond) * B`.
- **Variadic multi-controlled dispatch** via
  `generalizedInvokeWithRotationsControlsTargets` for the mapped
  (op, numControls, numTargets) tuples in
  `builders::resolve_controlled_gate_name`. Adjoint flag maps to
  `inv @`.
- CFG reduction: sequential blocks, if / if-else, single-exit while,
  short-circuit phi.
- `%Qubit` / `%Result` structs plus LLVM inline struct-by-value
  parameter types like `{ double*, i64 }` (the lowering used for
  `list[float]` arguments).
- **`alloca`/`bitcast`/`getelementptr`/`store`/`load` scalar
  constant folding** for the ``list[float]`` parameter pattern:
  eagerly-bound scalar values stashed in a local buffer fold back to
  numeric literals at gate-argument use sites, so `rx(angles[0], q[0])`
  with `angles=[0.1, 0.2]` emits `rx(0.1) q[0];`.

### Out of scope — must produce a clear error

- User-defined struct types other than `%Qubit` / `%Result` and inline
  struct-by-value literals.
- Irreducible CFGs, multi-entry loops, nested loops the reducer can't
  structure.
- Runtime qubit allocation (`__quantum__rt__qubit_allocate`). All
  qubits must be assigned statically via `inttoptr`.
- Value-typed `select` for floating-point (`select i1 %c, double A,
  double B`). OpenQASM 3 has no classical ternary in value position;
  split into `if`/`else` upstream. The integer analog
  `select i1 %c, iN A, iN B` IS supported via inline arithmetic.
- Loop-carried phi — integer counters merging across a true
  back-edge. If-merge phis (if-then-increment patterns) ARE supported.
- Controlled-gate combinations not on the mapped list (e.g. 3-control
  X, controlled-H). Decompose upstream, or add a mapping to
  `resolve_controlled_gate_name` in `builders.rs`.
- Variadic QIR intrinsics other than
  `generalizedInvokeWithRotationsControlsTargets`.
- QIR output-recording intrinsics
  (`__quantum__rt__result_record_output`,
  `__quantum__rt__array_record_output`) and the i1-load /
  store-to-alloca patterns some producers emit under the Base
  Profile for output labeling. Use the Adaptive Profile instead.


## Module map — where to go for each subsystem

For an up-to-date view of the source layout, run `ls
crates/qirtoqasm-core/src/` (or any other crate). Every Rust file
opens with a `//!` module-level doc comment that states its purpose;
read those rather than relying on this file. The high-level routing
guide below is what an agent typically needs:

**If you're changing…**

- …**how we parse QIR text**: `ir/parser.rs`. Accepts both
  opaque-pointer (`ptr null`) and typed-pointer (`%Qubit* null`) forms
  — any new operand shape must cover both.
- …**signature extraction** (what's a qubit vs result vs classical
  scalar): `signatures.rs`. Text-level regex — opaque pointers render
  as `ptr` so the typed form is authoritative for disambiguation.
- …**which functions get lowered and how**: `profile.rs` +
  `builders.rs`. `base_profile()` is the registry; `lower_call`
  dispatches.
- …**classical control flow** (icmp, select, phi, short-circuit
  booleans): `boolean.rs`.
- …**which CFG shapes are lowerable**: `cfg.rs`.
- …**OpenQASM 3 output formatting**: `oq3/ast.rs` + `oq3/printer.rs`.
  Byte-exact fixture-parity checks lock this down.
- …**Rust → Python bindings**: `crates/qirtoqasm-py/src/lib.rs`.
  PyO3 0.21, `Bound<'_, T>` API.
- …**C ABI**: `crates/qirtoqasm-ffi/src/lib.rs`. Keep the header in
  `include/qirtoqasm/qirtoqasm.h` in sync.


## Test layers

Four tiers, each with its own tox env.

1. **Rust inline unit tests** — `cargo test -p qirtoqasm-core --lib`.
   White-box tests of private helpers (`ssa_key`, `paren_depth`, CFG
   reduction internals) live inline as `#[cfg(test)] mod tests {}`.
2. **Rust integration tests** — `cargo test -p qirtoqasm-core -p
   qirtoqasm-ffi`. Public-API tests + fixture-parity byte compare
   (`crates/qirtoqasm-core/tests/fixture_parity.rs`) + FFI smoke.
3. **Python tests driven by tox**:
   - `tox -e unit-tests` — `test/unit_tests/` against the Rust-backed
     wheel. No Braket / Q# / CUDA-Q. 100% coverage enforced.
   - `tox -e integ-fixture-parity` — `.ll` → `.qasm` byte regression. No
     Braket.
   - `tox -e integ-braket` — end-to-end on Braket LocalSimulator.
   - `tox -e integ-qsharp` — Q# source → QIR (via the `qsharp`
     PyPI package) → qirtoqasm → Braket. Subprocess-isolated per check.
   - `tox -e integ-cudaq` — CUDA-Q kernel → QIR → qirtoqasm → Braket.
     Subprocess-isolated. Linux + macOS only.

### Subprocess isolation: why and how

`test_qsharp_e2e.py` and `test_cudaq_e2e.py` spawn a fresh Python
subprocess per check because:

- `qsharp.init(target_profile=...)` resets interpreter state
  process-wide.
- CUDA-Q's MLIR JIT carries process-wide state that can segfault on
  back-to-back kernel compilations.

The cudaq path uses `tempfile.NamedTemporaryFile` rather than
`python -c` because cudaq's `@cudaq.kernel` decorator calls
`inspect.getsourcelines`, which requires the kernel function to live
in an on-disk file.


## CI layout

All CI lives under `.github/workflows/`:

- **`ci.yml`** — main pipeline on pushes to `main` and PRs. Jobs:
  - `build` — cargo build/test + lint + `tox -e unit-tests` +
    `tox -e integ-fixture-parity`, 3 OS × 3 Py matrix. No Braket.
  - `msrv` — Rust 1.75 build + test on Ubuntu.
  - `coverage` — `cargo llvm-cov` ≥ 97% on `qirtoqasm-core`.
  - `integ-braket` — all three OSes × 3 Py.
  - `integ-qsharp` — all three OSes × 3 Py.
  - `integ-cudaq` — Linux + macOS × 3 Py (no Windows cudaq wheel).
  - `cpp-smoke` — CMake build + run of `cmake/cpp_smoke.cpp`
    (C++20, `-Wall -Wextra -Werror -Wpedantic` / MSVC `/W4 /WX
    /permissive-`) on Ubuntu + macOS + Windows.
  - `c-smoke` — CMake build + run of `cmake/c_smoke.c` (C11,
    same warnings-as-errors set) on the same 3-OS matrix.
    `integ-fixture-parity`, `integ-braket`, `integ-qsharp`,
    `integ-cudaq` all `needs: [build, cpp-smoke, c-smoke]`, so a
    broken native ABI blocks the Python integ tiers.
- **`wheels.yml`** — cibuildwheel. PR runs a single-platform smoke;
  release tags + publications build the full matrix (Linux
  manylinux2014 + musllinux_1_2 × x86_64 / aarch64, macOS x86_64 +
  arm64, Windows x86_64). One `cp311-abi3` wheel per platform covers
  Python 3.11 + 3.12 + 3.13.
- **`publish-to-pypi.yml`** — uploads `wheels.yml` artifacts via PyPI
  trusted publishing (OIDC) on release publication.
- **`twine-check.yml`** — `twine check` on every PR.
- **`check-format.yml`** — `tox -e linters-check` on every PR.
- **`code-freeze.yml`** — gates merges during release-freeze windows
  using `vars.FROZEN` / `vars.UNFROZEN_PREFIX`.
- **`dependabot.yml`** — weekly GitHub Actions + Cargo (grouped
  minor/patch) + pip updates.


## Release process

1. Branch and bump the version:
   ```bash
   git checkout -b release/v0.X.Y
   # edit _version.py to 0.X.Y (drop the .devN suffix)
   python scripts/sync_version.py
   ```
2. Open a PR, let CI go green, merge.
3. Draft a GitHub release with tag `v0.X.Y`. Publishing triggers
   `wheels.yml` (full matrix) + `publish-to-pypi.yml` (upload via
   PyPI trusted publishing).
4. Bump to the next dev version on `main`:
   ```bash
   # edit _version.py to 0.X.(Y+1).dev0
   python scripts/sync_version.py
   ```


## Gotchas

### `let ... else` for test destructuring

The Rust codebase uses `let Pat = expr else { panic!("context") };` in
tests to assert an enum variant while extracting its fields. Prefer
this over `match expr { Pat => ..., _ => panic!() }`: the wildcard arm
is dead coverage mass that llvm-cov counts against you forever.

### `#[cfg(test)]` module placement

White-box tests of private helpers live inline inside each `src/*.rs`
file as `#[cfg(test)] mod tests { ... }`. The `tests/` directory at
`crates/qirtoqasm-core/tests/` is reserved for integration tests
against the public API (`fixture_parity.rs`, `known_limitations.rs`).

### `anyio` pytest plugin

`anyio`'s pytest plugin installs transitively via Jupyter. Its
`pytest_pyfunc_call` hook can crash the interpreter during error
formatting. `-p no:anyio` is set in `[tool.pytest.ini_options]` and
every tox env. If you see segfaults inside `_pytest/_code/code.py`
frames, check that `-p no:anyio` is still in force.

### Opaque vs typed pointer operands

The parser (`ir/parser.rs::parse_operand`) accepts both opaque-pointer
form (`ptr null`, `ptr inttoptr (i64 N to ptr)`) and typed-pointer
form (`%Qubit* null`, `%Qubit* inttoptr (i64 N to %Qubit*)`).
Different QIR producers emit different forms; any new operand-shape
support must cover both.

### SSA value keying (`ssa_key`)

A naive `symbols.ssa[value.name] = ...` keys every numerically-named
SSA value (`%1`, `%2`) on `""` since those have empty `.name`, so
every later `record_ssa` call silently overwrites the previous one.
The fix is `ssa_key(name, text)` in `symbols.rs`:

1. If `name` is non-empty (`%cond`, `%tmp.1`), use it.
2. Otherwise, parse the leading `%<id>` token from the instruction's
   text form.
3. Otherwise, fall back to the full text.

Both `record_ssa` and `lookup_ssa` route through `ssa_key` so they
agree. Regression tests in `symbols::tests` cover the 4-iteration
IPE pattern that tickles this edge case.

### Variadic function signatures

LLVM permits a trailing `...` in a function signature. `FunctionSignature`
records this as `is_variadic: bool`. When a call dispatches and the
callee is variadic, the translator raises `QirToQasmError::Unsupported`
with a message that names the callee. If a future profile adds a
specialized builder for a variadic intrinsic, the check in
`translator.rs` needs updating — today `signature.param_types` covers
only the fixed prefix.

### Braket accepts integer comparisons, not `true`/`false`

Braket's local simulator types `c[N]` as an integer in expression
contexts and rejects `c[N] == true` with "mixed int/bool comparison".
We always emit integer literal comparisons: `c[N] == 0`, `c[N] == 1`.
The `boolean::as_boolean_expression` helper normalizes a raw integer
expression into `<expr> == 1` so compound `&&` / `||` conditions
evaluate. See the regression test at
`boolean::tests::phi_and_pattern_binds_compound_with_eq_wrapping`.

### `qirtoqasm-py` is excluded from `cargo build --workspace`

The Cargo workspace's `default-members` excludes `qirtoqasm-py`
because its pyo3 cdylib needs maturin-supplied link flags; a plain
`cargo build --workspace` would try to link it standalone and fail on
macOS with undefined Python symbols. Build the extension via
`maturin develop` or `maturin build`. Plain `cargo build` works
because it uses `default-members`.

### `rust-toolchain.toml` pins `1.82.0`

Pinned (not rolling `stable`) so that dev machines, cibuildwheel, and
CI agree on lint output. Bumping it is a deliberate change that may
need to absorb new clippy / rustfmt findings.

### Python shim coverage is literal 100%

The shim at `python/qirtoqasm/__init__.py` is a tiny re-export. Adding
any code there means extending tests to keep coverage at 100% (the
CI floor is 100% via `--cov-fail-under=100` in `tox.ini`).


