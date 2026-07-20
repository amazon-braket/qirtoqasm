OPENQASM 3.0;
qubit[2] q;
bit[2] c;
xx(0.5) q[0], q[1];
yy(0.3) q[0], q[1];
zz(0.2) q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
