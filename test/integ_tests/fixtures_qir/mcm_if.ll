; Mid-circuit measurement with classical feed-forward: if the first
; qubit measures 1, X-flip the second before measuring it.
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %1 = call i1 @__quantum__qis__read_result__body(%Result* null)
  br i1 %1, label %true_branch, label %join

true_branch:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %join

join:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }
