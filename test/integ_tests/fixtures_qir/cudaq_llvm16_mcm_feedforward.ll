; Canonical MCM + single-branch feedforward produced by CUDA-Q's qir-adaptive codegen
; (LLVM 16). Logically equivalent to cudaq_mcm_feedforward.ll but uses LLVM 15+
; textual-IR conventions: quoted numeric block labels (`"0":`, `"1":`, `"2":`) and
; opaque-ptr operands everywhere. Should lower to the same OpenQASM 3 program.
; ModuleID = 'qir-bitcode'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@cstr.6D00 = private constant [2 x i8] c"m\00"
@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"
@cstr.61727261793C6931207820323E00 = private constant [14 x i8] c"array<i1 x 2>\00"

define void @__nvqpp__mlirgen__function_mcm_feedforward._Z15mcm_feedforwardv() local_unnamed_addr #0 {
"0":
  tail call void @__quantum__qis__h__body(ptr null)
  tail call void @__quantum__qis__mz__body(ptr null, ptr writeonly null)
  tail call void @__quantum__rt__array_record_output(i64 2, ptr nonnull @cstr.61727261793C6931207820323E00)
  tail call void @__quantum__rt__result_record_output(ptr null, ptr nonnull @cstr.6D00)
  %0 = tail call i1 @__quantum__qis__read_result__body(ptr null)
  br i1 %0, label %"1", label %"2"

"1":                                              ; preds = %"0"
  tail call void @__quantum__qis__x__body(ptr nonnull inttoptr (i64 1 to ptr))
  br label %"2"

"2":                                              ; preds = %"1", %"0"
  tail call void @__quantum__qis__mz__body(ptr nonnull inttoptr (i64 1 to ptr), ptr nonnull writeonly inttoptr (i64 1 to ptr))
  tail call void @__quantum__rt__result_record_output(ptr nonnull inttoptr (i64 1 to ptr), ptr nonnull @cstr.72303030303000)
  ret void
}

declare void @__quantum__qis__h__body(ptr) local_unnamed_addr

declare void @__quantum__qis__x__body(ptr) local_unnamed_addr

declare void @__quantum__qis__mz__body(ptr, ptr writeonly) local_unnamed_addr #1

declare void @__quantum__rt__result_record_output(ptr, ptr) local_unnamed_addr

declare i1 @__quantum__qis__read_result__body(ptr) local_unnamed_addr

declare void @__quantum__rt__array_record_output(i64, ptr) local_unnamed_addr

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22m\22]],[1,[1,\22r00000\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9, !10, !11, !12}

!0 = !{i32 2, !"Debug Info Version", i32 3}
!1 = !{i32 1, !"qir_major_version", i32 0}
!2 = !{i32 7, !"qir_minor_version", i32 1}
!3 = !{i32 1, !"dynamic_qubit_management", i1 false}
!4 = !{i32 1, !"dynamic_result_management", i1 false}
!5 = !{i32 1, !"qubit_resetting", i1 true}
!6 = !{i32 1, !"classical_ints", i1 false}
!7 = !{i32 1, !"classical_floats", i1 false}
!8 = !{i32 1, !"classical_fixed_points", i1 false}
!9 = !{i32 1, !"user_functions", i1 false}
!10 = !{i32 1, !"dynamic_float_args", i1 false}
!11 = !{i32 1, !"extern_functions", i1 false}
!12 = !{i32 1, !"backwards_branching", i1 false}
