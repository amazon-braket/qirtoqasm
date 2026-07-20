/// # Summary
/// 4-bit iterative phase estimation. Extracts the phase `phi = 2*pi * 3/16`
/// one bit at a time using a single reusable ancilla (q[0]) and a data
/// qubit (q[1]). Each iteration's outcome is copied into a storage qubit
/// (q[2]..q[5]) so the full result is visible in the end-of-circuit
/// bitstring — Braket's local simulator returns only final qubit states,
/// not the mid-circuit measurement history.
///
/// # Expected result
/// For `phi = 2*pi * 3/16`, the phase bits (LSB first) are
/// `b0=1, b1=1, b2=0, b3=0`. Every shot deterministically recovers that
/// bit pattern in the storage register.
///
/// # References
/// Svore et al., *Faster Phase Estimation*, arXiv:1304.0741 (2013).
/// Córcoles et al., arXiv:2102.01682 (2021).
///
/// # Profile
/// Requires `TargetProfile.Adaptive_RI`.
import Std.Math.*;

operation CPhase(theta : Double, control : Qubit, target : Qubit) : Unit is Adj + Ctl {
    Rz(theta / 2.0, target);
    CNOT(control, target);
    Rz(-theta / 2.0, target);
    CNOT(control, target);
    Rz(theta / 2.0, control);
}

operation IPE4Bit() : Result {
    // Layout:
    //   q[0]      -- ancilla (reused each iteration)
    //   q[1]      -- data qubit (eigenstate of the oracle)
    //   q[2..5]   -- storage for b0..b3 (observed in final counts)
    use q = Qubit[6];

    let phase = 2.0 * PI() * 3.0 / 16.0;

    // Prepare the oracle eigenstate on the data qubit.
    X(q[1]);

    // ---- Iteration 0 (LSB) ----
    // Oracle applied 2^3 = 8 times, no correction.
    H(q[0]);
    for _ in 1..8 { CPhase(phase, q[0], q[1]); }
    H(q[0]);
    let b0 = M(q[0]);
    if (b0 == One) { X(q[2]); }
    Reset(q[0]);

    // ---- Iteration 1 ----
    // Oracle applied 4 times; correct using b0.
    H(q[0]);
    if (b0 == One) { Rz(-PI() / 2.0, q[0]); }
    for _ in 1..4 { CPhase(phase, q[0], q[1]); }
    H(q[0]);
    let b1 = M(q[0]);
    if (b1 == One) { X(q[3]); }
    Reset(q[0]);

    // ---- Iteration 2 ----
    // Oracle applied 2 times; correct using b0, b1.
    H(q[0]);
    if (b0 == One) { Rz(-PI() / 4.0, q[0]); }
    if (b1 == One) { Rz(-PI() / 2.0, q[0]); }
    for _ in 1..2 { CPhase(phase, q[0], q[1]); }
    H(q[0]);
    let b2 = M(q[0]);
    if (b2 == One) { X(q[4]); }
    Reset(q[0]);

    // ---- Iteration 3 (MSB) ----
    // Oracle applied once; correct using b0, b1, b2.
    H(q[0]);
    if (b0 == One) { Rz(-PI() / 8.0, q[0]); }
    if (b1 == One) { Rz(-PI() / 4.0, q[0]); }
    if (b2 == One) { Rz(-PI() / 2.0, q[0]); }
    CPhase(phase, q[0], q[1]);
    H(q[0]);
    let b3 = M(q[0]);
    if (b3 == One) { X(q[5]); }

    return b3;
}
