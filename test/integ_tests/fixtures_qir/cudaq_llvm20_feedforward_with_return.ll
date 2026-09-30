; Return-typed MCM + feedforward kernel produced by CUDA-Q's qir-adaptive
; codegen (LLVM 20). Logically equivalent to cudaq_feedforward_with_return.ll,
; but the `list[bool]` return value is lowered differently: instead of reusing
; the `i1` values already produced by `read_result`, the result indices are
; materialized into an `[2 x i64]` stack array, loaded back, and turned into
; `%Result*` values by standalone `inttoptr` instructions. The `captures(none)`
; parameter attribute (in place of `nocapture`) marks the LLVM 20 textual-IR
; convention. Should lower to the same OpenQASM 3 program.
source_filename = "LLVMDialectModule"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"

@cstr.623000 = private constant [3 x i8] c"b0\00"
@cstr.623100 = private constant [3 x i8] c"b1\00"
@cstr.61727261793C6931207820323E00 = private constant [14 x i8] c"array<i1 x 2>\00"

define { ptr, i64 } @__nvqpp__mlirgen__kernel..0x7fde9e71d400() #0 {
  call void @__quantum__qis__h__body(ptr null)
  call void @__quantum__qis__mz__body(ptr null, ptr null)
  call void @__quantum__rt__array_record_output(i64 2, ptr @cstr.61727261793C6931207820323E00)
  call void @__quantum__rt__result_record_output(ptr null, ptr @cstr.623000)
  %1 = call i1 @__quantum__qis__read_result__body(ptr null)
  br i1 %1, label %2, label %3

2:                                                ; preds = %0
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  br label %3

3:                                                ; preds = %2, %0
  call void @__quantum__qis__mz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @cstr.623100)
  %4 = alloca [2 x i64], align 8
  store i64 0, ptr %4, align 8
  %5 = getelementptr [2 x i64], ptr %4, i32 0, i32 1
  store i64 1, ptr %5, align 8
  %6 = alloca [2 x i8], align 1
  %7 = load i64, ptr %4, align 8
  %8 = inttoptr i64 %7 to ptr
  %9 = call i1 @__quantum__qis__read_result__body(ptr %8)
  %10 = zext i1 %9 to i8
  store i8 %10, ptr %6, align 1
  %11 = load i64, ptr %5, align 8
  %12 = inttoptr i64 %11 to ptr
  %13 = call i1 @__quantum__qis__read_result__body(ptr %12)
  %14 = getelementptr [2 x i8], ptr %6, i32 0, i32 1
  %15 = zext i1 %13 to i8
  store i8 %15, ptr %14, align 1
  %16 = call ptr @malloc(i64 2)
  call void @llvm.memcpy.p0.p0.i64(ptr %16, ptr %6, i64 2, i1 false)
  %17 = insertvalue { ptr, i64 } undef, ptr %16, 0
  %18 = insertvalue { ptr, i64 } %17, i64 2, 1
  ret { ptr, i64 } %18
}

; Function Attrs: nocallback nofree nounwind willreturn memory(argmem: readwrite)
declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly captures(none), ptr noalias readonly captures(none), i64, i1 immarg) #1

declare ptr @malloc(i64)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__mz__body(ptr, ptr) #2

declare void @__quantum__rt__result_record_output(ptr, ptr)

declare i1 @__quantum__qis__read_result__body(ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22b0\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { nocallback nofree nounwind willreturn memory(argmem: readwrite) }
attributes #2 = { "irreversible" }

!llvm.module.flags = !{!0}

!0 = !{i32 2, !"Debug Info Version", i32 3}
