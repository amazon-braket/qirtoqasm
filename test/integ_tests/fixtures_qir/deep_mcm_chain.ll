; Three sequential mid-circuit measurements, each feeding-forward into
; the next qubit. Exercises the CFG reducer's ability to chain multiple
; independent if-without-else reductions in sequence.
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %b0 = call i1 @__quantum__qis__read_result__body(%Result* null)
  br i1 %b0, label %t0, label %j0
t0:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %j0
j0:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %b1 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %b1, label %t1, label %j1
t1:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %j1
j1:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="3" "requiredResults"="3" }
attributes #1 = { "irreversible" }
