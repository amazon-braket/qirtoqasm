/// Sim-safe GHZ-5: measures + ResetAll so the Q# simulator
/// can release qubits cleanly. Semantically identical to ghz5.qs.
operation GHZ5Sim() : Result[] {
    use q = Qubit[5];
    H(q[0]);
    CNOT(q[0], q[1]);
    CNOT(q[1], q[2]);
    CNOT(q[2], q[3]);
    CNOT(q[3], q[4]);
    let r = [M(q[0]), M(q[1]), M(q[2]), M(q[3]), M(q[4])];
    ResetAll(q);
    return r;
}
