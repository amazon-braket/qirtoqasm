# Contributing Guidelines

Thank you for your interest in contributing to qirtoqasm. Whether it's
a bug report, new feature, correction, or additional documentation, we
greatly value feedback and contributions from our community.

Please read through this document before submitting any issues or
pull requests to ensure we have all the necessary information to
effectively respond to your bug report or contribution.


## Table of Contents

* [Report Bugs/Feature Requests](#report-bugsfeature-requests)
* [Contribute via Pull Requests (PRs)](#contribute-via-pull-requests-prs)
  * [Pull Down the Code](#pull-down-the-code)
  * [Run the Tests](#run-the-tests)
  * [Make and Test Your Change](#make-and-test-your-change)
  * [Commit Your Change](#commit-your-change)
  * [Send a Pull Request](#send-a-pull-request)
* [Documentation Guidelines](#documentation-guidelines)
* [Find Contributions to Work On](#find-contributions-to-work-on)
* [Code of Conduct](#code-of-conduct)
* [Security Issue Notifications](#security-issue-notifications)
* [Licensing](#licensing)


## Report Bugs/Feature Requests

We welcome you to use the GitHub issue tracker to report bugs or
suggest features.

When filing an issue, please check [existing open](https://github.com/amazon-braket/qirtoqasm/issues)
and [recently closed](https://github.com/amazon-braket/qirtoqasm/issues?utf8=%E2%9C%93&q=is%3Aissue%20is%3Aclosed%20)
issues to make sure somebody else hasn't already reported the issue.
Please try to include as much information as you can. Details like
these are incredibly useful:

* A reproducible test case or series of steps.
* If the bug is in the QIR → OpenQASM translation itself, the smallest
  `.ll` input that reproduces it plus the expected output.
* The version of our code being used.
* Any modifications you've made relevant to the bug.
* A description of your environment or deployment.


## Contribute via Pull Requests (PRs)

Contributions via pull requests are much appreciated.

Before sending us a pull request, please ensure that:

* You are working against the latest source on the *main* branch.
* You check the existing open and recently merged pull requests to
  make sure someone else hasn't already addressed the problem.
* You open an issue to discuss any significant work — we would hate
  for your time to be wasted.


### Pull Down the Code

1. If you do not already have one, create a GitHub account by following
   the prompts at [Join GitHub](https://github.com/join).
1. Create a fork of this repository on GitHub. You should end up with
   a fork at `https://github.com/<username>/qirtoqasm`.
   1. Follow the instructions at
      [Fork a Repo](https://help.github.com/en/articles/fork-a-repo).
1. Clone your fork: `git clone https://github.com/<username>/qirtoqasm`
   where `<username>` is your GitHub username.


### Run the Tests

qirtoqasm is a Rust workspace plus a PyO3-built Python package plus a
CMake-installed native library, all driven by tox for end-to-end
consistency.

1. Create and activate a Python 3.11 environment (conda, venv, or
   similar).
1. Install [Rust stable](https://rustup.rs/) and the build tools:
   ```bash
   pip install 'maturin>=1.5,<2.0' tox
   ```
1. Build the Rust-backed wheel and install it editable:
   ```bash
   maturin develop --release
   ```
1. Run the tests:
   ```bash
   # Full pre-PR suite (lint + docs + unit + integ-fixture-parity +
   # integ-braket + integ-qsharp + integ-cudaq).
   tox

   # Individual tiers:
   tox -e unit-tests            # unit tests only
   tox -e integ-fixture-parity  # .ll / .qasm regression suite (no Braket)
   tox -e integ-braket          # qirtoqasm → Braket LocalSimulator
   tox -e integ-qsharp          # Q# → qirtoqasm → Braket
   tox -e integ-cudaq           # CUDA-Q → qirtoqasm → Braket (Linux/macOS only)
   ```

You can pass pytest arguments through tox with `--`, e.g.
`tox -e unit-tests -- -k test_bell -v`.


### Make and Test Your Change

1. Create a new git branch:
    ```shell
    git checkout -b my-fix-branch main
    ```
1. Make your changes, **including unit tests** and, if appropriate,
   integration tests.
   1. Include unit tests when you contribute new features or make bug
      fixes, as they help to:
      1. Prove that your code works correctly.
      1. Guard against future breaking changes to lower the maintenance
         cost.
   1. Please focus on the specific change you are contributing. If you
      also reformat all the code, it will be hard for us to focus on
      your change.
1. If your change affects the Rust core, also run
   `cargo test -p qirtoqasm-core -p qirtoqasm-ffi`, `cargo fmt --check`,
   and `cargo clippy --all-targets -- -D warnings`.
1. Run `tox` to verify that all checks and tests pass.
1. If your change bumps the version, edit **only**
   `python/qirtoqasm/_version.py` and run
   `python scripts/sync_version.py` to propagate the new version to
   `pyproject.toml` and `Cargo.toml`. The `tox -e linters` env enforces
   this.


### Commit Your Change

We use commit messages to update the project version number and
generate changelog entries, so it's important for them to follow the
right format. Valid commit messages include a prefix, separated from
the rest of the message by a colon and a space. Here are a few
examples:

```
feature: support new QIR intrinsic `__quantum__qis__<foo>__body`
fix: fix phi-chain lowering in CFG reducer
documentation: clarify supported QIR constructs
infra: bump cibuildwheel version in wheels.yml
```

Valid prefixes are listed in the table below.

| Prefix          | Use for...                                                                                     |
|----------------:|:-----------------------------------------------------------------------------------------------|
| `breaking`      | Incompatible API changes.                                                                      |
| `deprecation`   | Deprecating an existing API or feature, or removing something that was previously deprecated.  |
| `feature`       | Adding a new feature.                                                                          |
| `fix`           | Bug fixes.                                                                                     |
| `change`        | Any other code change.                                                                         |
| `documentation` | Documentation changes.                                                                         |
| `infra`         | CI / build / tooling changes.                                                                  |

Some of the prefixes allow abbreviation; `break`, `feat`, `depr`, and
`doc` are all valid. If you omit a prefix, the commit will be treated
as a `change`.

For the rest of the message, use imperative style and keep things
concise but informative. See [How to Write a Git Commit Message](https://chris.beams.io/posts/git-commit/)
for guidance.


### Send a Pull Request

GitHub provides additional documentation on
[Creating a Pull Request](https://help.github.com/articles/creating-a-pull-request/).

Please remember to:

* Use commit messages (and PR titles) that follow the guidelines under
  [Commit Your Change](#commit-your-change).
* Send us a pull request, answering any default questions in the pull
  request interface.
* Pay attention to any automated CI failures reported in the pull
  request, and stay involved in the conversation.


## Documentation Guidelines

Our documentation is built with [Sphinx](https://www.sphinx-doc.org/)
using the [sphinx-rtd-theme](https://sphinx-rtd-theme.readthedocs.io/)
and hosted on [Read the Docs](https://qirtoqasm.readthedocs.io/). The
build uses `sphinxcontrib-apidoc` to auto-generate API reference pages
from the docstrings on the Python shim.

Prose documentation files use **reStructuredText (`.rst`)** format and
live in the `doc/` directory. For a quick primer on RST syntax, see
[the Sphinx documentation](https://www.sphinx-doc.org/en/main/usage/restructuredtext/basics.html).

### API References (docstrings)

Rust source files use `///` rustdoc comments; Python files use
[Google-style docstrings](https://sphinxcontrib-napoleon.readthedocs.io/en/latest/example_google.html)
for the shim-level classes and functions. (The PyO3 extension's own
docstrings come from the `#[pyo3(text_signature = "...")]` attributes
and `#[doc = "..."]` annotations on the Rust side.)

When possible, link to classes and functions, e.g. use
":func:\`qirtoqasm.translate\`" over just "translate".

### Build and Test Documentation

```shell
tox -e docs
```

The generated HTML lands under `build/documentation/html`. Serve it
locally with `tox -e serve-docs 8080` and open
http://localhost:8080/index.html.


## Find Contributions to Work On

Looking at the existing issues is a great way to find something to
contribute on. As our projects, by default, use the default GitHub
issue labels (enhancement/bug/duplicate/help wanted/invalid/question/
wontfix), looking at any ['help wanted'](https://github.com/amazon-braket/qirtoqasm/labels/help%20wanted)
issues is a great place to start.


## Code of Conduct

This project has adopted the
[Amazon Open Source Code of Conduct](https://aws.github.io/code-of-conduct).
For more information see the
[Code of Conduct FAQ](https://aws.github.io/code-of-conduct-faq) or
contact opensource-codeofconduct@amazon.com with any additional
questions or comments.


## Security Issue Notifications

If you discover a potential security issue in this project we ask that
you notify AWS/Amazon Security via our
[vulnerability reporting page](http://aws.amazon.com/security/vulnerability-reporting/).
Please do **not** create a public GitHub issue.


## Licensing

See the [LICENSE](https://github.com/amazon-braket/qirtoqasm/blob/main/LICENSE)
file for our project's licensing. We will ask you to confirm the
licensing of your contribution.

We may ask you to sign a
[Contributor License Agreement (CLA)](http://en.wikipedia.org/wiki/Contributor_License_Agreement)
for larger changes.
