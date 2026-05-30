API reference
=============

The ``qirtoqasm`` Python package exposes a single one-stage translation
function. The package is a thin shim over a Rust core; the PyO3
extension module ``qirtoqasm._qirtoqasm_native`` contains the
implementation, and the public surface is re-exported at the package
top level for convenience.


Translation
-----------

.. autofunction:: qirtoqasm.translate


Exceptions
----------

.. autoexception:: qirtoqasm.QIRTOQASMError


Version
-------

.. autodata:: qirtoqasm.__version__
