; Q# `CountExceeding` — Adaptive_RI integer accumulation plus a
; threshold branch (`if total >= 2 { X(out); }`). Exercises both
; chained phi-i64 lowering and integer `icmp sge` comparison of the
; accumulated count against a constant.
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_r\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %var_4 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_4, label %block_1, label %block_2
block_1:
  br label %block_2
block_2:
  %var_16 = phi i64 [0, %block_0], [1, %block_1]
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_6 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_6, label %block_3, label %block_4
block_3:
  %var_8 = add i64 %var_16, 1
  br label %block_4
block_4:
  %var_17 = phi i64 [%var_16, %block_2], [%var_8, %block_3]
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_9 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_9, label %block_5, label %block_6
block_5:
  %var_11 = add i64 %var_17, 1
  br label %block_6
block_6:
  %var_18 = phi i64 [%var_17, %block_4], [%var_11, %block_5]
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  %var_12 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_12, label %block_7, label %block_8
block_7:
  %var_14 = add i64 %var_18, 1
  br label %block_8
block_8:
  %var_19 = phi i64 [%var_18, %block_6], [%var_14, %block_7]
  %var_15 = icmp sge i64 %var_19, 2
  br i1 %var_15, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  br label %block_10
block_10:
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
attributes #1 = { "irreversible" }
