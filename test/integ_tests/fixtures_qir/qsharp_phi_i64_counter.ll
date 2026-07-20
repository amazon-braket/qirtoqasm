; Q# `count_ones` — Adaptive_RI integer accumulation. Compiles to
; QIR with chained `phi i64` merges across if-no-else blocks, each
; bumping the counter by 1 when the measured bit is |1>.
%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_3 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_3, label %block_1, label %block_2
block_1:
  br label %block_2
block_2:
  %var_12 = phi i64 [0, %block_0], [1, %block_1]
  %var_5 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_5, label %block_3, label %block_4
block_3:
  %var_7 = add i64 %var_12, 1
  br label %block_4
block_4:
  %var_13 = phi i64 [%var_12, %block_2], [%var_7, %block_3]
  %var_8 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_8, label %block_5, label %block_6
block_5:
  %var_10 = add i64 %var_13, 1
  br label %block_6
block_6:
  %var_14 = phi i64 [%var_13, %block_4], [%var_10, %block_5]
  call void @__quantum__rt__int_record_output(i64 %var_14, i8* null)
  ret i64 0
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
declare void @__quantum__rt__int_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
attributes #1 = { "irreversible" }
