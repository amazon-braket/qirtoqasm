OPENQASM 3.0;
qubit[4] q;
bit[4] c;
h q[0];
h q[1];
h q[2];
c[0] = measure q[0];
c[1] = measure q[1];
c[2] = measure q[2];
if (c[0] == 1 && c[1] == 1 && c[2] == 1) {
  x q[3];
}
c[3] = measure q[3];
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"adaptive_profile"}
