OPENQASM 3.0;
qubit[3] q;
bit[3] c;
h q[0];
c[0] = measure q[0];
if (c[0]) {
  x q[1];
}
h q[1];
c[1] = measure q[1];
if (c[1]) {
  x q[2];
}
c[2] = measure q[2];
