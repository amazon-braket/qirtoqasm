OPENQASM 3.0;
qubit[4] q;
bit[2] c;
h q[0];
cnot q[0], q[1];
cnot q[0], q[2];
cnot q[1], q[3];
c[0] = measure q[0];
c[1] = measure q[1];
