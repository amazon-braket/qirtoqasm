OPENQASM 3.0;
qubit[1] q;
bit[2] c;
h q[0];
c[0] = measure q[0];
reset q[0];
x q[0];
c[1] = measure q[0];
