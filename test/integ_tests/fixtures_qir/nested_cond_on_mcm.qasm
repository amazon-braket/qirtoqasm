OPENQASM 3.0;
qubit[3] q;
bit[5] c;
h q[0];
c[0] = measure q[0];
if (c[0]) {
  x q[1];
}
h q[2];
c[1] = measure q[2];
if (c[1]) {
  z q[1];
}
c[2] = measure q[0];
c[3] = measure q[1];
c[4] = measure q[2];
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"adaptive_profile"}
