/// Sim-safe Grover2: measures + ResetAll so the Q# simulator
/// can release qubits cleanly. Semantically identical to grover2.qs.
operation Grover2Sim() : Result[] {
    use q = Qubit[2];
    H(q[0]);
    H(q[1]);
    // Oracle: mark |11>
    CZ(q[0], q[1]);
    // Diffuser
    H(q[0]);
    H(q[1]);
    X(q[0]);
    X(q[1]);
    CZ(q[0], q[1]);
    X(q[0]);
    X(q[1]);
    H(q[0]);
    H(q[1]);
    let r = [M(q[0]), M(q[1])];
    ResetAll(q);
    return r;
}
