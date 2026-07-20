/// Sim-safe teleportation: returns all three measurement results and
/// resets qubits. Semantically identical to teleport.qs.
operation TeleportSim() : (Result, Result, Result) {
    use (msg, here, target) = (Qubit(), Qubit(), Qubit());
    H(msg);
    H(here);
    CNOT(here, target);
    CNOT(msg, here);
    H(msg);
    let m1 = M(msg);
    let m2 = M(here);
    if (m2 == One) { X(target); }
    if (m1 == One) { Z(target); }
    H(target);
    let rt = M(target);
    Reset(msg);
    Reset(here);
    Reset(target);
    return (m1, m2, rt);
}
