%Qubit = type opaque
%Result = type opaque

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  br label %16
16:
  %17 = call i1 @__quantum__qis__read_result__body(%Result* null)
  br i1 %17, label %26, label %21
21:
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  br label %16
26:
  br label %52
52:
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* inttoptr (i64 1 to %Result*))
  ret void
}

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="1" "requiredResults"="2" }
attributes #1 = { "irreversible" }
