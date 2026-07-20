operation Bell() : (Result, Result) {
    use q = Qubit[2];
    H(q[0]);
    CNOT(q[0], q[1]);
    return (M(q[0]), M(q[1]));
}
