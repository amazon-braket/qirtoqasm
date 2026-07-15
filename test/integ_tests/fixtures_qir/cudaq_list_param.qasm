OPENQASM 3.0;
qubit[2] q;
bit[2] c;
rx(0.1) q[0];
ry(0.2) q[1];
cnot q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
