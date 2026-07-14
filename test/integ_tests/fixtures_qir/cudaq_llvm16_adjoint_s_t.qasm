OPENQASM 3.0;
qubit[2] q;
bit[2] c;
rz(1.5707963267948966) q[0];
rz(-1.5707963267948966) q[0];
rz(0.7853981633974483) q[1];
rz(-0.7853981633974483) q[1];
c[0] = measure q[0];
c[1] = measure q[1];
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"adaptive_profile"}
