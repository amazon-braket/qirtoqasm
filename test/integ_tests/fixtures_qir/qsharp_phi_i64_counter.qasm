OPENQASM 3.0;
qubit[3] q;
bit[3] c;
int cint_0 = 0;
h q[0];
h q[1];
h q[2];
c[0] = measure q[0];
c[1] = measure q[1];
c[2] = measure q[2];
if (c[0]) {
  cint_0 = 1;
}
if (c[1]) {
  cint_0 = cint_0 + 1;
}
if (c[2]) {
  cint_0 = cint_0 + 1;
}
