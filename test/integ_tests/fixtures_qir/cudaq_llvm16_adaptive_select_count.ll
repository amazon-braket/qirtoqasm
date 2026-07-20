; CUDA-Q native `--target braket` optimized adaptive-profile emission
; for the `adaptive_repeat_count.cpp` kernel (count one-bits from 3
; measurements, apply X if count >= 2). Post-opt-pass LLVM collapses
; the would-be `phi i32` merge chain into a `select i1 %c, i32 A, i32 B`
; cascade plus `zext` / `add` / `icmp ugt` — the shape qirtoqasm lowers
; to inline arithmetic `(cond) * A + (1 - cond) * B`.
; ModuleID = 'qir-bitcode'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@cstr.62253000 = private constant [4 x i8] c"b%0\00"
@cstr.62253100 = private constant [4 x i8] c"b%1\00"
@cstr.62253200 = private constant [4 x i8] c"b%2\00"
@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"
@cstr.61727261793C6931207820343E00 = private constant [14 x i8] c"array<i1 x 4>\00"

define void @__nvqpp__mlirgen__function_adaptive_repeat_count._Z21adaptive_repeat_countv() local_unnamed_addr #0 {
"0":
  tail call void @__quantum__qis__h__body(ptr null)
  tail call void @__quantum__qis__mz__body(ptr null, ptr writeonly null)
  tail call void @__quantum__rt__array_record_output(i64 4, ptr nonnull @cstr.61727261793C6931207820343E00)
  tail call void @__quantum__rt__result_record_output(ptr null, ptr nonnull @cstr.62253000)
  %0 = tail call i1 @__quantum__qis__read_result__body(ptr null)
  %1 = zext i1 %0 to i32
  tail call void @__quantum__qis__h__body(ptr nonnull inttoptr (i64 1 to ptr))
  tail call void @__quantum__qis__mz__body(ptr nonnull inttoptr (i64 1 to ptr), ptr nonnull writeonly inttoptr (i64 1 to ptr))
  tail call void @__quantum__rt__result_record_output(ptr nonnull inttoptr (i64 1 to ptr), ptr nonnull @cstr.62253100)
  %2 = tail call i1 @__quantum__qis__read_result__body(ptr nonnull inttoptr (i64 1 to ptr))
  %3 = select i1 %0, i32 2, i32 1
  %spec.select = select i1 %2, i32 %3, i32 %1
  tail call void @__quantum__qis__h__body(ptr nonnull inttoptr (i64 2 to ptr))
  tail call void @__quantum__qis__mz__body(ptr nonnull inttoptr (i64 2 to ptr), ptr nonnull writeonly inttoptr (i64 2 to ptr))
  tail call void @__quantum__rt__result_record_output(ptr nonnull inttoptr (i64 2 to ptr), ptr nonnull @cstr.62253200)
  %4 = tail call i1 @__quantum__qis__read_result__body(ptr nonnull inttoptr (i64 2 to ptr))
  %5 = zext i1 %4 to i32
  %6 = add nuw nsw i32 %spec.select, %5
  %7 = icmp ugt i32 %6, 1
  br i1 %7, label %"1", label %"2"

"1":                                              ; preds = %"0"
  tail call void @__quantum__qis__x__body(ptr nonnull inttoptr (i64 3 to ptr))
  br label %"2"

"2":                                              ; preds = %"1", %"0"
  tail call void @__quantum__qis__mz__body(ptr nonnull inttoptr (i64 3 to ptr), ptr nonnull writeonly inttoptr (i64 3 to ptr))
  tail call void @__quantum__rt__result_record_output(ptr nonnull inttoptr (i64 3 to ptr), ptr nonnull @cstr.72303030303000)
  ret void
}

declare void @__quantum__qis__h__body(ptr) local_unnamed_addr

declare void @__quantum__qis__x__body(ptr) local_unnamed_addr

declare void @__quantum__qis__mz__body(ptr, ptr writeonly) local_unnamed_addr #1

declare void @__quantum__rt__result_record_output(ptr, ptr) local_unnamed_addr

declare i1 @__quantum__qis__read_result__body(ptr) local_unnamed_addr

declare void @__quantum__rt__array_record_output(i64, ptr) local_unnamed_addr

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22b%0\22]],[1,[1,\22b%1\22]],[2,[2,\22b%2\22]],[3,[3,\22r00000\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="4" "requiredResults"="4" }
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
