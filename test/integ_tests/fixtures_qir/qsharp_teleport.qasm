OPENQASM 3.0;
qubit[3] q;
bit[3] c;
h q[0];
h q[1];
cnot q[1], q[2];
cnot q[0], q[1];
h q[0];
c[0] = measure q[0];
c[1] = measure q[1];
if (c[1]) {
  x q[2];
}
if (c[0]) {
  z q[2];
}
h q[2];
c[2] = measure q[2];
