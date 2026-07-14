OPENQASM 3.0;
qubit[2] q;
bit[2] c;
s q[0];
inv @ s q[0];
t q[1];
inv @ t q[1];
c[0] = measure q[0];
c[1] = measure q[1];
// generated-by: {"name":"qirtoqasm","version":"0.1.0-dev0","profile":"adaptive_profile"}
