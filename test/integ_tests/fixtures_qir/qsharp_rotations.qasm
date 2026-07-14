OPENQASM 3.0;
qubit[1] q;
bit[1] c;
rx(0.5) q[0];
ry(1.0) q[0];
rz(1.5) q[0];
c[0] = measure q[0];
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"base_profile"}
