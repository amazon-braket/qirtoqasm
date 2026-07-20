/// # Summary
/// 2-qubit Grover's search with a marked state. With n=2 we only need
/// a single iteration of Grover's; the amplitude on the marked state
/// becomes 1 and measurement is deterministic.
///
/// We hard-code the marked state to |11> = 3. The Grover oracle
/// flips the sign of the |11> component; the diffuser then reflects
/// about the uniform superposition. For n=2 the theory gives an
/// angle pi/2, so the amplitude on the marked state is already 1
/// after one iteration.
///
/// # Structure
/// Clean separation of oracle and diffuser as helper operations,
/// matching Microsoft's Grover sample on qdk docs.

operation MarkedOracle(q : Qubit[]) : Unit is Adj + Ctl {
    // Mark |11>: apply a Z on the |11> component via CZ.
    // For 2 qubits we can use Controlled-Z(q[0]; q[1]).
    CZ(q[0], q[1]);
}

operation Diffuser(q : Qubit[]) : Unit is Adj + Ctl {
    // Reflection about the uniform superposition.
    // The standard implementation:
    //   H * X^n * (controlled-Z) * X^n * H
    ApplyToEachCA(H, q);
    ApplyToEachCA(X, q);
    CZ(q[0], q[1]);
    ApplyToEachCA(X, q);
    ApplyToEachCA(H, q);
}

operation ApplyToEachCA<'T>(op : ('T => Unit is Adj + Ctl), targets : 'T[]) : Unit is Adj + Ctl {
    // Q#'s standard library has this as `Std.Canon.ApplyToEachCA`, but
    // we write it out so the .qs file is self-contained.
    for t in targets {
        op(t);
    }
}

operation Grover2() : Result[] {
    use q = Qubit[2];
    // Uniform superposition.
    ApplyToEachCA(H, q);

    // One Grover iteration.
    MarkedOracle(q);
    Diffuser(q);

    return [M(q[0]), M(q[1])];
}
