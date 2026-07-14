OPENQASM 3.0;
qubit[3] q;
bit[3] c;
h q[0];
cnot q[0], q[1];
cnot q[1], q[2];
c[0] = measure q[0];
c[1] = measure q[1];
c[2] = measure q[2];
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"base_profile"}
