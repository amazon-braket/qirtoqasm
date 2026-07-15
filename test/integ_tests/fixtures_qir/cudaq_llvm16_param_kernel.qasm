OPENQASM 3.0;
qubit[1] q;
bit[1] c;
rx(0.5) q[0];
c[0] = measure q[0];
