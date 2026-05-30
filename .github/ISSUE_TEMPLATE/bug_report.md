---
name: Bug report
about: File a report to help us reproduce and fix the problem
title: ''
labels: 'bug'
assignees: ''

---

**Describe the bug**
A clear and concise description of what the bug is.

**To reproduce**
A clear, step-by-step set of instructions to reproduce the bug. If the
bug is in the QIR → OpenQASM translation itself, please include the
smallest ``.ll`` input that reproduces the issue, plus the expected
OpenQASM 3 output.

**Expected behavior**
A clear and concise description of what you expected to happen.

**Screenshots or logs**
If applicable, add screenshots or logs to help explain your problem.

**System information**
A description of your system. Please provide:
- **qirtoqasm version**:
- **Python version**:
- **Rust toolchain version** (``rustc --version``, if built from source):
- **OS** (Linux / macOS / Windows, and distribution / version):
- **Installation method**: (``pip install qirtoqasm`` / ``maturin develop`` / from source / etc.)

**Upstream producer (if relevant)**
If the QIR input came from Q#, CUDA-Q, or another compiler, please
specify its name and version so we can reproduce with a matching
input. Attach a minimal program if possible.

**Additional context**
Add any other context about the problem here.
