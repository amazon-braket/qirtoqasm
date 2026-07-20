; Parametric 2-qubit rotations — common in QAOA-style and Trotterized
; Ising simulations. Tests that the rxx / ryy / rzz registrations lower
; correctly.
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__rxx__body(double 0.3, %Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__ryy__body(double 0.5, %Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.7, %Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  ret void
}

declare void @__quantum__qis__rxx__body(double, %Qubit*, %Qubit*)
declare void @__quantum__qis__ryy__body(double, %Qubit*, %Qubit*)
declare void @__quantum__qis__rzz__body(double, %Qubit*, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1

attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }
