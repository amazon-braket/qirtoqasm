/// # Summary
/// Quantum teleportation using two mid-circuit measurements and
/// classical feedforward. Prepares the message qubit in |+>, runs
/// the standard teleportation protocol, and then applies H on the
/// target before re-measuring. If teleportation succeeded, the
/// target always measures Zero.
///
/// # Profile
/// Requires `TargetProfile.Adaptive_RI`.
operation Teleport() : Result {
    // Alice has (msg, here), Bob has (target).
    use (msg, here, target) = (Qubit(), Qubit(), Qubit());

    // Prepare the message qubit in |+>.
    H(msg);

    // Share an entangled pair between Alice and Bob.
    H(here);
    CNOT(here, target);

    // Alice performs a Bell-basis measurement on (msg, here).
    CNOT(msg, here);
    H(msg);
    let m1 = M(msg);
    let m2 = M(here);

    // Bob applies corrections based on Alice's classical bits.
    if (m2 == One) { X(target); }
    if (m1 == One) { Z(target); }

    // Verify: apply H to convert |+> back to |0>. Deterministic 0
    // if teleportation succeeded.
    H(target);
    return M(target);
}
