/// Sim-safe feedforward: measures + ResetAll so the Q# simulator
/// can release qubits cleanly. Semantically identical to feedforward.qs.
operation FeedforwardSim() : (Result, Result) {
    use q = Qubit[2];
    H(q[0]);
    let b0 = M(q[0]);
    if (b0 == One) {
        X(q[1]);
    }
    let r1 = M(q[1]);
    ResetAll(q);
    return (b0, r1);
}
