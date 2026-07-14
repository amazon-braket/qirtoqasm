%Qubit = type opaque
%Result = type opaque

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__z__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %6 = call i1 @__quantum__qis__read_result__body(%Result* null)
  br i1 %6, label %16, label %19
16:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %20
19:
  br label %20
20:
  br label %21
21:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %25 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %25, label %35, label %38
35:
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %39
38:
  br label %39
39:
  br label %40
40:
  br label %66
66:
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  ret void
}

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="3" "requiredResults"="5" }
attributes #1 = { "irreversible" }
