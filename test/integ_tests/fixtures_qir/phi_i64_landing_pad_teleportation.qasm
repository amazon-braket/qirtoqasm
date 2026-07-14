OPENQASM 3.0;
qubit[3] q;
bit[2] c;
h q[1];
cnot q[1], q[2];
cnot q[0], q[1];
h q[0];
c[0] = measure q[0];
c[1] = measure q[1];
if (c[1] == 1) {
  x q[2];
}
if (c[0] == 1) {
  z q[2];
}
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"adaptive_profile"}
