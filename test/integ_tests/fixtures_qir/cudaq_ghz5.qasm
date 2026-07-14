OPENQASM 3.0;
qubit[5] q;
bit[5] c;
h q[0];
cnot q[0], q[1];
cnot q[1], q[2];
cnot q[2], q[3];
cnot q[3], q[4];
c[0] = measure q[0];
c[1] = measure q[1];
c[2] = measure q[2];
c[3] = measure q[3];
c[4] = measure q[4];
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"adaptive_profile"}
