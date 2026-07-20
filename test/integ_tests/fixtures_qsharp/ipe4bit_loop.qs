/// # Summary
/// 4-bit iterative phase estimation, expressed with an outer `for` loop
/// over the four iterations rather than the hand-unrolled body in
/// `ipe4bit.qs`. Same target, same deterministic bitstring.
///
/// This fixture exercises the adaptive-profile lowering of:
///   * an integer-indexed outer `for` loop (compile-time unrolled by Q#)
///   * a mutable `Result[]` accumulator read across iterations
///   * inner `for _ in 1..count` oracle loops where `count` is a
///     compile-time-known power of two
///   * classical feedforward on the accumulator bits (one `if` per
///     previously-measured bit per correction)
///
/// The phase is `2*pi * 3/16` so the recovered bit pattern (LSB first)
/// is `1, 1, 0, 0`; every shot deterministically reproduces it in the
/// storage register.
///
/// # Profile
/// Requires `TargetProfile.Adaptive_RI`.
import Std.Math.*;
import Std.Convert.IntAsDouble;

operation CPhase(theta : Double, control : Qubit, target : Qubit) : Unit is Adj + Ctl {
    Rz(theta / 2.0, target);
    CNOT(control, target);
    Rz(-theta / 2.0, target);
    CNOT(control, target);
    Rz(theta / 2.0, control);
}

operation IPE4BitLoop() : Result {
    // Layout:
    //   q[0]      -- ancilla (reused each iteration)
    //   q[1]      -- data qubit (eigenstate of the oracle)
    //   q[2..5]   -- storage for b0..b3 (observed in final counts)
    use q = Qubit[6];

    let phase = 2.0 * PI() * 3.0 / 16.0;

    // Prepare the oracle eigenstate on the data qubit.
    X(q[1]);

    // Holds the previously measured phase bits, LSB first.
    mutable bits = [Zero, size = 4];

    for i in 0..3 {
        // Rounds still to resolve after this one, i.e. how many times
        // to apply the oracle. Compile-time-known integer so Q# unrolls
        // the inner loop.
        let rounds_left = 3 - i;
        let oracle_count = 1 <<< rounds_left;

        H(q[0]);

        // Correction: for each previously-measured bit bits[k] (k < i),
        // if it was One, subtract pi / 2^(i - k) from the ancilla phase.
        // Expressed as one conditional Rz per prior bit so the inner
        // angle stays a compile-time Double.
        for k in 0..(i - 1) {
            let correction = -PI() / IntAsDouble(1 <<< (i - k));
            if (bits[k] == One) {
                Rz(correction, q[0]);
            }
        }

        for _ in 1..oracle_count {
            CPhase(phase, q[0], q[1]);
        }

        H(q[0]);
        let b = M(q[0]);
        set bits w/= i <- b;

        // Copy the result into the storage register so it's visible in
        // the final shot bitstring.
        if (b == One) {
            X(q[2 + i]);
        }

        // Reset the ancilla for the next iteration (skip on the final
        // round — the ancilla is no longer reused).
        if (i < 3) {
            Reset(q[0]);
        }
    }

    return bits[3];
}
