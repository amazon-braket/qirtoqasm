; Reference input exercising the variadic-multi-controlled qirtoqasm code
; path with `numControls=2`, in LLVM 16 textual-IR form (opaque `ptr`,
; quoted numeric block labels). This is the opaque-ptr analogue of
; `cudaq_toffoli.ll` (typed-pointer form) and is not what the current
; CUDA-Q basis-gate pass actually emits for `x<cudaq::ctrl>(a,b,c)` -
; with `x(1)` in the basis, CUDA-Q pre-decomposes CCX into 6 CNOTs + 7
; rotations before qirtoqasm runs. The fixture ensures that IF such a
; configuration emits a Toffoli-shaped `generalizedInvoke` (e.g. after
; a basis change, or from another QIR producer targeting the same
; variadic ABI), qirtoqasm lowers it to the Braket-native `ccnot`.
; ModuleID = 'qir-bitcode'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"
@cstr.72303030303100 = private constant [7 x i8] c"r00001\00"
@cstr.72303030303200 = private constant [7 x i8] c"r00002\00"
@cstr.61727261793C6931207820333E00 = private constant [14 x i8] c"array<i1 x 3>\00"

define void @__nvqpp__mlirgen__function_toffoli._Z7toffoliv() local_unnamed_addr #0 {
"0":
  tail call void @__quantum__qis__h__body(ptr null)
  tail call void @__quantum__qis__h__body(ptr nonnull inttoptr (i64 1 to ptr))
  tail call void (i64, i64, i64, i64, ptr, ...) @generalizedInvokeWithRotationsControlsTargets(i64 0, i64 0, i64 2, i64 1, ptr nonnull @__quantum__qis__x__ctl, ptr null, ptr nonnull inttoptr (i64 1 to ptr), ptr nonnull inttoptr (i64 2 to ptr))
  tail call void @__quantum__qis__mz__body(ptr null, ptr writeonly null)
  tail call void @__quantum__rt__array_record_output(i64 3, ptr nonnull @cstr.61727261793C6931207820333E00)
  tail call void @__quantum__rt__result_record_output(ptr null, ptr nonnull @cstr.72303030303000)
  %0 = tail call i1 @__quantum__qis__read_result__body(ptr null)
  tail call void @__quantum__qis__mz__body(ptr nonnull inttoptr (i64 1 to ptr), ptr nonnull writeonly inttoptr (i64 1 to ptr))
  tail call void @__quantum__rt__result_record_output(ptr nonnull inttoptr (i64 1 to ptr), ptr nonnull @cstr.72303030303100)
  %1 = tail call i1 @__quantum__qis__read_result__body(ptr nonnull inttoptr (i64 1 to ptr))
  tail call void @__quantum__qis__mz__body(ptr nonnull inttoptr (i64 2 to ptr), ptr nonnull writeonly inttoptr (i64 2 to ptr))
  tail call void @__quantum__rt__result_record_output(ptr nonnull inttoptr (i64 2 to ptr), ptr nonnull @cstr.72303030303200)
  %2 = tail call i1 @__quantum__qis__read_result__body(ptr nonnull inttoptr (i64 2 to ptr))
  ret void
}

declare void @__quantum__qis__h__body(ptr) local_unnamed_addr
declare void @__quantum__qis__x__ctl(ptr, ptr) local_unnamed_addr
declare void @generalizedInvokeWithRotationsControlsTargets(i64, i64, i64, i64, ptr, ...) local_unnamed_addr
declare void @__quantum__qis__mz__body(ptr, ptr writeonly) local_unnamed_addr #1
declare void @__quantum__rt__result_record_output(ptr, ptr) local_unnamed_addr
declare i1 @__quantum__qis__read_result__body(ptr) local_unnamed_addr
declare void @__quantum__rt__array_record_output(i64, ptr) local_unnamed_addr

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22r00000\22]],[1,[1,\22r00001\22]],[2,[2,\22r00002\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="3" "requiredResults"="3" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0}

!0 = !{i32 2, !"Debug Info Version", i32 3}
