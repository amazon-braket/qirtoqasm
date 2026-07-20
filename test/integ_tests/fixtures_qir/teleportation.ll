; Quantum teleportation: Bell pair + classical feedforward on both
; measurement outcomes. Exercises two sequential conditional branches.
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %b1 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %b1, label %apply_x, label %skip_x
apply_x:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %skip_x
skip_x:
  %b0 = call i1 @__quantum__qis__read_result__body(%Result* null)
  br i1 %b0, label %apply_z, label %done
apply_z:
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %done
done:
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__z__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="3" "requiredResults"="2" }
attributes #1 = { "irreversible" }
