OPENQASM 3.0;
qubit[1] q;
bit[2] c;
h q[0];
c[0] = measure q[0];
while (c[0] == 0) {
  h q[0];
  c[0] = measure q[0];
}
c[1] = measure q[0];
