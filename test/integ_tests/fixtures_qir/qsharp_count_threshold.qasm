OPENQASM 3.0;
qubit[5] q;
bit[5] c;
int cint_0 = 0;
h q[0];
h q[1];
h q[2];
h q[3];
c[0] = measure q[0];
if (c[0]) {
  cint_0 = 1;
}
c[1] = measure q[1];
if (c[1]) {
  cint_0 = cint_0 + 1;
}
c[2] = measure q[2];
if (c[2]) {
  cint_0 = cint_0 + 1;
}
c[3] = measure q[3];
if (c[3]) {
  cint_0 = cint_0 + 1;
}
if (cint_0 >= 2) {
  x q[4];
}
c[4] = measure q[4];
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"adaptive_profile"}
