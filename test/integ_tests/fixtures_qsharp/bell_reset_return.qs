/// Sim-safe Bell state: measures + ResetAll so the Q# simulator
/// can release qubits cleanly. Semantically identical to bell.qs.
operation BellSim() : (Result, Result) {
    use q = Qubit[2];
    H(q[0]);
    CNOT(q[0], q[1]);
    let r0 = M(q[0]);
    let r1 = M(q[1]);
    ResetAll(q);
    return (r0, r1);
}
