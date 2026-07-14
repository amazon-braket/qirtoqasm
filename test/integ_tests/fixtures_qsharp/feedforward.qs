operation Feedforward() : (Result, Result) {
    use q = Qubit[2];
    H(q[0]);
    let b0 = M(q[0]);
    if (b0 == One) {
        X(q[1]);
    }
    return (b0, M(q[1]));
}
