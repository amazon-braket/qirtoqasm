operation GHZ5() : Result[] {
    use q = Qubit[5];
    H(q[0]);
    CNOT(q[0], q[1]);
    CNOT(q[1], q[2]);
    CNOT(q[2], q[3]);
    CNOT(q[3], q[4]);
    return [M(q[0]), M(q[1]), M(q[2]), M(q[3]), M(q[4])];
}
