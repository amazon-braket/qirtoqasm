# qirtoqasm: QIR to OpenQASM translator

[![Latest Version](https://img.shields.io/pypi/v/qirtoqasm.svg)](https://pypi.python.org/pypi/qirtoqasm)
[![Supported Python Versions](https://img.shields.io/pypi/pyversions/qirtoqasm.svg)](https://pypi.python.org/pypi/qirtoqasm)
[![Build status](https://github.com/amazon-braket/qirtoqasm/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/amazon-braket/qirtoqasm/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/amazon-braket/qirtoqasm/graph/badge.svg)](https://codecov.io/gh/amazon-braket/qirtoqasm)
[![Documentation Status](https://readthedocs.org/projects/qirtoqasm/badge/?version=latest)](https://qirtoqasm.readthedocs.io/en/latest/?badge=latest)

**qirtoqasm is not an officially supported AWS product.**

This experimental library translates [QIR](https://github.com/qir-alliance/qir-spec)
programs (the [QIR Base Profile](https://github.com/qir-alliance/qir-spec/blob/main/specification/under_development/profiles/Base_Profile.md)
and [Adaptive Profile](https://github.com/qir-alliance/qir-spec/blob/main/specification/under_development/profiles/Adaptive_Profile.md))
to **Braket-compatible [OpenQASM 3.0](https://openqasm.com)**. qirtoqasm
is _experimental_ software. We may change, remove, or deprecate parts
of the qirtoqasm API without notice.


## Why qirtoqasm?

qirtoqasm bridges the growing ecosystem of **QIR-emitting quantum
compilers** to the **OpenQASM 3** format that Amazon Braket accepts.
Many quantum frontends emit QIR as their serialization format —
qirtoqasm converts that QIR to a Braket-ready OpenQASM 3 program
without any further rewriting required by the caller.

The implementation is a **pure-Rust core** with four public faces:

- A **Python package** (`qirtoqasm`) built via PyO3 + maturin. Single
  one-stage public function: `qirtoqasm.translate(qir_text,
  *, producer=None)` returns Braket-compatible OpenQASM 3. Every
  tunable is a keyword-only kwarg, so new options can be added
  without breaking existing callers.
- A **Rust crate** (`qirtoqasm-core`) exposing `translate(qir_text,
  &TranslateOptions)` plus a `#[non_exhaustive]` options struct with
  builder methods.
- A **C ABI** (`libqirtoqasm.{a,dylib,so}`) whose
  `qirtoqasm_translate(qir, &options, out, err)` takes a versioned
  `qirtoqasm_options_t` struct (carrying its own `struct_version` /
  `struct_size` so fields can be appended without breaking existing
  callers). Pass `NULL` options for defaults.
- A **C++20 header** (`qirtoqasm/qirtoqasm.hpp`) with `translate(qir)`
  and `translate(qir, const Options&)`, designed for C++20
  designated-initializer syntax.

No LLVM, llvmlite, or any Python runtime dependency — the only
external dependency is the platform C runtime. The Python wheel is
self-contained.


## Braket-targeted by design

The output of `qirtoqasm.translate` can be handed directly to
`braket.ir.openqasm.Program` — no further rewriting required. Several
emit-name choices follow from this:

- Two-qubit Ising rotations emit as `xx` / `yy` / `zz` (Braket
  aliases), not the OpenQASM `stdgates.inc` names `rxx` / `ryy` /
  `rzz`.
- CNOT emits as `cnot` (not `cx`).
- Toffoli emits as `ccnot` (not `ccx`).
- Classical registers are declared as plain `bit[N] c;`, without the
  OpenQASM `output` qualifier.
- No `include "stdgates.inc";` is emitted.


## Installation

```bash
pip install qirtoqasm
```

Python ≥ 3.11 is required. The wheel is self-contained — no Rust or
LLVM is needed at install time. Wheels are published for Linux
(manylinux x86_64 / aarch64 + musllinux x86_64), macOS (x86_64 +
arm64), and Windows (x86_64).


## Python quick start

```python
import qirtoqasm

qasm = qirtoqasm.translate("""
    %Qubit = type opaque
    %Result = type opaque
    define void @main() #0 {
      call void @__quantum__qis__h__body(%Qubit* null)
      call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
      call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      ret void
    }
    declare void @__quantum__qis__h__body(%Qubit*)
    declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
    declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
    attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="2" }
    attributes #1 = { "irreversible" }
""")
print(qasm)
```

`qirtoqasm.translate` accepts QIR as a string. To read from a file,
use the standard library:

```python
from pathlib import Path
qasm = qirtoqasm.translate(Path("bell.ll").read_text())
```

Every output ends with a trailing
`// generated-by: {"name":"qirtoqasm",…}` comment. Callers that wrap
qirtoqasm inside a larger toolchain can pass an optional keyword-only
`producer` string to surface their own tool name and version in the
comment:

```python
qasm = qirtoqasm.translate(ir_text, producer="mylib 0.1.2")
# last line: // generated-by: {"name":"qirtoqasm","version":"…","profile":"base_profile","producer":"mylib 0.1.2"}
```

### Submitting to Amazon Braket

```python
from pathlib import Path
from braket.devices import LocalSimulator
from braket.ir.openqasm import Program

program = Program(source=qirtoqasm.translate(Path("bell.ll").read_text()))
result = LocalSimulator().run(program, shots=1000).result()
print(result.measurement_counts)
```

### Input format

qirtoqasm accepts QIR as **LLVM textual IR (`.ll` text)** only. LLVM
bitcode (`.bc`) is not supported; if you have bitcode, convert it with
`llvm-dis` first. This is deliberate: the Rust core has no LLVM link
dependency, which keeps the wheel tiny (no 100+ MB LLVM payload) and
keeps the build hermetic.


## C++ quick start

Requires a C++20 compiler. The public surface is `qirtoqasm::translate`
plus a small `Options` struct for tunables:

```cpp
#include <qirtoqasm/qirtoqasm.hpp>

// Simplest: defaults.
std::string qasm = qirtoqasm::translate(qir_text);

// With options (C++20 designated initializers):
std::string qasm = qirtoqasm::translate(qir_text,
    qirtoqasm::Options{ .producer = "mylib 0.1.2" });
// throws qirtoqasm::TranslationError on failure
```

From CMake:

```cmake
find_package(qirtoqasm REQUIRED)
target_link_libraries(my_target PRIVATE qirtoqasm::qirtoqasm)
target_compile_features(my_target PRIVATE cxx_std_20)
```

## C quick start

For C-only consumers, the same C ABI is exposed via a header:

```c
#include <qirtoqasm/qirtoqasm.h>

char *out = NULL, *err = NULL;

// Simplest: NULL options uses defaults.
if (qirtoqasm_translate(qir_text, NULL, &out, &err) != QIRTOQASM_OK) { /* … */ }

// With options — always call qirtoqasm_options_init first so future
// fields inherit correct defaults:
qirtoqasm_options_t opts;
qirtoqasm_options_init(&opts);
opts.producer = "mylib 0.1.2";
if (qirtoqasm_translate(qir_text, &opts, &out, &err) != QIRTOQASM_OK) { /* … */ }

qirtoqasm_free_string(out);
qirtoqasm_free_string(err);
```

See [DEVELOPMENT.md](DEVELOPMENT.md) for the native-library build
workflow.


## Supported QIR constructs

- QIS gates that match the `__quantum__qis__<name>__body` naming
  convention: `h`, `x`, `y`, `z`, `s`, `t`, `cnot`/`cx`, `cy`, `cz`,
  `swap`, `rx`, `ry`, `rz`, `ccx`/`ccnot`, `rxx`, `ryy`, `rzz`,
  `reset`, measurement (`mz` / `m` / `mresetz`), `phasedx`, and the
  `__adj` adjoints for non-self-adjoint gates.
- Mid-circuit measurement read via
  `__quantum__qis__read_result__body(%Result*)` and the alias
  `__quantum__rt__read_result`, driving conditional `br i1`.
- **Compound Boolean conditions.** `icmp <pred> i1/iN` with all ten
  LLVM integer predicates (`eq`, `ne`, `ult`/`slt`, `ule`/`sle`,
  `ugt`/`sgt`, `uge`/`sge`); direct bitwise i1 `and`, `or`, `xor`
  (including the `xor i1 %x, true` logical-NOT idiom); `select i1`
  (including clang's short-circuit shapes `select %c, %b, false` and
  `select %c, true, %b`); and `phi i1` short-circuit merges. Covers
  common frontends' short-circuit encodings and the compound-
  Boolean forms clang emits for `a && b`, `a || b`, `!a`, `a == b`,
  `a != b`.
- **Integer arithmetic** on classical SSA values: `add`, `sub`, `mul`
  (with the `nuw`/`nsw`/`exact` overflow-flag tokens allowed). The
  resulting expression is inlined at the use site, so the operands
  must resolve to classical-register reads, integer constants, or
  previously bound arithmetic.
- **`phi i32` / `phi i64` integer accumulation.** The
  `mutable count = 0; if r == One { set count = count + 1; }` pattern
  compiles to chained two-incoming phis. The translator lowers the
  chain to a single OpenQASM 3 `int cint_N = 0;` classical variable
  plus conditional `cint_N = cint_N + 1;` assignments, plus
  `if (cint_N >= T) { … }` threshold branches via `icmp`.
- **`select i1 %c, iN A, iN B` integer cascade.** Optimized QIR
  produced by LLVM's opt pipeline collapses the integer-accumulation
  chain into nested `select`s plus `zext` / `add` / `icmp ugt` on an
  integer counter. Lowers to the inline arithmetic form
  `(cond) * A + (1 - cond) * B`; downstream `add` / `icmp` flow
  through the expanded expression.
- **`alloca` / `bitcast` / `getelementptr` / `store` / `load`
  scalar constant folding** for the common `list[float]` parameter
  pattern: eagerly-bound scalar values stashed in a local buffer get
  folded back to their numeric literals at the gate-argument use
  site, so `rx(angles[0], q[0])` with `angles=[0.1, 0.2]` emits
  `rx(0.1) q[0];`.
- **Variadic multi-controlled dispatch** via the
  `generalizedInvokeWithRotationsControlsTargets` intrinsic lowers to
  the matching Braket-native gate for the following inner callees:
  `__quantum__qis__x__ctl` with 1 or 2 controls (→ `cnot` / `ccnot`),
  `y__ctl` / `z__ctl` with 1 control (→ `cy` / `cz`),
  `swap__ctl` with 1 control and 2 targets (→ `cswap`), and
  `phaseshift__ctl` with 1 control (→ `cphaseshift`). The adjoint
  flag maps to `inv @`. Unmapped `(op, numControls, numTargets)`
  tuples produce a descriptive error pointing at upstream
  decomposition.
- CFG reduction: sequential blocks, if / if-else, single-exit while
  loops (one back-edge), and short-circuit phi merges.
- Structs `%Qubit` and `%Result`, plus inline struct-by-value
  parameter literals (`{ double*, i64 }`). Other user-defined struct
  types raise `QirToQasmError`.


## Out of scope (produce a clear error)

All unsupported cases raise `qirtoqasm.QirToQasmError` with a message
naming the root cause.

- Nested or irreducible CFGs, multi-entry loops, nested loops the
  reducer cannot structure.
- Runtime qubit allocation (`__quantum__rt__qubit_allocate`). All
  qubits must be assigned statically via `inttoptr`.
- **Value-typed `select` for floating-point** (`select i1 %c, double
  A, double B` used to choose a rotation angle). OpenQASM 3 has no
  classical ternary in value position; split into `if`/`else` arms
  upstream or precompute. The integer analog `select i1 %c, iN A,
  iN B` IS supported via inline arithmetic.
- **Loop-carried phi** — integer counters or booleans merging across
  a true back-edge loop latch. If-merge phis are supported.
- **Controlled-gate combinations not on the mapped list** above
  (e.g. 3-control X, controlled-H). Decompose upstream, or open an
  issue with the missing mapping.


## Contributing and sharing feedback

We welcome feature requests, bug reports, or general feedback, which
you can share with us by
[opening up an issue](https://github.com/amazon-braket/qirtoqasm/issues/new/choose).
We also welcome pull requests — please open an issue describing your
work when you get started, or comment on an existing issue with your
intentions. For more details on contributing to qirtoqasm, please read
the [contributing guidelines](CONTRIBUTING.md).

For questions, you can get help via the Quantum Technologies section
of [AWS RePost](https://repost.aws/topics/TAxin6L9GYR5a3NElq8AHIqQ/quantum-technologies).
Please tag your question with "Amazon Braket" and mention qirtoqasm in
the question title.


## Tests

To run qirtoqasm's unit tests, run:

```bash
tox -e unit-tests
```

See [DEVELOPMENT.md](DEVELOPMENT.md) and the [docs site](https://qirtoqasm.readthedocs.io/)
for the full workflow (Rust build, maturin, CMake, coverage, all tox
environments).

## Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## License

This project is licensed under the Apache-2.0 License.

