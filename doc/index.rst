qirtoqasm
=========

Translate `QIR <https://github.com/qir-alliance/qir-spec>`_ programs
(the QIR Base Profile and Adaptive Profile) to Braket-compatible
`OpenQASM 3.0 <https://openqasm.com>`_.

The implementation is a pure-Rust core with four public faces:

- A **Python package** (``qirtoqasm``) built via PyO3 + maturin. Single
  public function ``qirtoqasm.translate(qir_text, *, producer=None)``
  with keyword-only options.
- A **Rust crate** (``qirtoqasm-core``) exposing
  ``translate(qir_text, &TranslateOptions)`` plus a
  ``#[non_exhaustive]`` options struct with builder methods.
- A **C ABI** (``libqirtoqasm.{a,dylib,so}``) whose
  ``qirtoqasm_translate`` takes a versioned ``qirtoqasm_options_t``
  struct; pass ``NULL`` for defaults.
- A **C++20 header** (``qirtoqasm/qirtoqasm.hpp``) with designated
  initializer syntax: ``qirtoqasm::translate(qir, { .producer = "…" })``.

The output is ready to submit to Amazon Braket — the translator emits
Braket-native gate names (``cnot`` / ``ccnot`` / ``xx`` / ``yy`` /
``zz``) and plain ``bit[N] c;`` declarations, so no further rewriting
is required.


Installation
------------

.. code-block:: bash

   pip install qirtoqasm

Python ≥ 3.11 is required. The wheel is self-contained — no Rust or
LLVM is needed at install time.


Quick start
-----------

.. code-block:: python

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


``qirtoqasm.translate`` accepts QIR as a string. To read from a file,
use the standard library:

.. code-block:: python

   from pathlib import Path
   qasm = qirtoqasm.translate(Path("bell.ll").read_text())


Every output ends with a trailing
``// generated-by: {"name":"qirtoqasm",…}`` comment. Callers wrapping
qirtoqasm inside a larger toolchain can pass an optional keyword-only
``producer`` string to surface their own tool and version in that
comment:

.. code-block:: python

   qasm = qirtoqasm.translate(ir_text, producer="mylib 0.1.2")


Using the output with Amazon Braket:

.. code-block:: python

   from pathlib import Path
   from braket.devices import LocalSimulator
   from braket.ir.openqasm import Program

   program = Program(source=qirtoqasm.translate(Path("bell.ll").read_text()))
   result = LocalSimulator().run(program, shots=1000).result()
   print(result.measurement_counts)


Contents
--------

.. toctree::
   :maxdepth: 2

   api
   development


Indices and tables
------------------

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search`
