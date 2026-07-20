/// Sim-safe 4-bit IPE: measures all qubits at end and resets.
/// Semantically identical to ipe4bit.qs but returns full register.
import Std.Math.*;

operation CPhase_Sim(theta : Double, control : Qubit, target : Qubit) : Unit is Adj + Ctl {
    Rz(theta / 2.0, target);
    CNOT(control, target);
    Rz(-theta / 2.0, target);
    CNOT(control, target);
    Rz(theta / 2.0, control);
}

operation IPE4BitSim() : Result[] {
    use q = Qubit[6];
    let phase = 2.0 * PI() * 3.0 / 16.0;
    X(q[1]);
    H(q[0]);
    for _ in 1..8 { CPhase_Sim(phase, q[0], q[1]); }
    H(q[0]);
    let b0 = M(q[0]);
    if (b0 == One) { X(q[2]); }
    Reset(q[0]);
    H(q[0]);
    if (b0 == One) { Rz(-PI() / 2.0, q[0]); }
    for _ in 1..4 { CPhase_Sim(phase, q[0], q[1]); }
    H(q[0]);
    let b1 = M(q[0]);
    if (b1 == One) { X(q[3]); }
    Reset(q[0]);
    H(q[0]);
    if (b0 == One) { Rz(-PI() / 4.0, q[0]); }
    if (b1 == One) { Rz(-PI() / 2.0, q[0]); }
    for _ in 1..2 { CPhase_Sim(phase, q[0], q[1]); }
    H(q[0]);
    let b2 = M(q[0]);
    if (b2 == One) { X(q[4]); }
    Reset(q[0]);
    H(q[0]);
    if (b0 == One) { Rz(-PI() / 8.0, q[0]); }
    if (b1 == One) { Rz(-PI() / 4.0, q[0]); }
    if (b2 == One) { Rz(-PI() / 2.0, q[0]); }
    CPhase_Sim(phase, q[0], q[1]);
    H(q[0]);
    let b3 = M(q[0]);
    if (b3 == One) { X(q[5]); }
    let r = [M(q[0]), M(q[1]), M(q[2]), M(q[3]), M(q[4]), M(q[5])];
    ResetAll(q);
    return r;
}
