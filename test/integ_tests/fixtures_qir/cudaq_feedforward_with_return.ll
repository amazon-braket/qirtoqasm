; ModuleID = 'LLVMDialectModule'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"

%Qubit = type opaque
%Result = type opaque

@cstr.623000 = private constant [3 x i8] c"b0\00"
@cstr.623100 = private constant [3 x i8] c"b1\00"
@cstr.61727261793C6931207820323E00 = private constant [14 x i8] c"array<i1 x 2>\00"
@cstr.5B305D00 = private constant [4 x i8] c"[0]\00"
@cstr.5B315D00 = private constant [4 x i8] c"[1]\00"

declare i8* @malloc(i64)

declare void @free(i8*)

define { i1*, i64 } @__nvqpp__mlirgen__kernel..0x1089303b0() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([14 x i8], [14 x i8]* @cstr.61727261793C6931207820323E00, i32 0, i32 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([3 x i8], [3 x i8]* @cstr.623000, i32 0, i32 0))
  %1 = call i1 @__quantum__qis__read_result__body(%Result* null)
  br i1 %1, label %2, label %3

2:                                                ; preds = %0
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %3

3:                                                ; preds = %2, %0
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([3 x i8], [3 x i8]* @cstr.623100, i32 0, i32 0))
  %4 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %5 = alloca [2 x i8], align 1
  %6 = bitcast [2 x i8]* %5 to i8*
  %7 = zext i1 %1 to i8
  store i8 %7, i8* %6, align 1
  %8 = getelementptr [2 x i8], [2 x i8]* %5, i32 0, i32 1
  %9 = zext i1 %4 to i8
  store i8 %9, i8* %8, align 1
  %10 = call i8* @malloc(i64 2)
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %10, i8* %6, i64 2, i1 false)
  %11 = bitcast i8* %10 to i1*
  %12 = insertvalue { i1*, i64 } undef, i1* %11, 0
  %13 = insertvalue { i1*, i64 } %12, i64 2, 1
  ret { i1*, i64 } %13
}

; Function Attrs: nocallback nofree nounwind willreturn memory(argmem: readwrite)
declare void @llvm.memcpy.p0i8.p0i8.i64(i8* noalias nocapture writeonly, i8* noalias nocapture readonly, i64, i1 immarg) #1

define void @__nvqpp__mlirgen__kernel..0x1089303b0.run() #2 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %1 = call i1 @__quantum__qis__read_result__body(%Result* null)
  br i1 %1, label %2, label %3

2:                                                ; preds = %0
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %3

3:                                                ; preds = %2, %0
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %4 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %5 = alloca [2 x i8], align 1
  %6 = bitcast [2 x i8]* %5 to i8*
  %7 = zext i1 %1 to i8
  store i8 %7, i8* %6, align 1
  %8 = getelementptr [2 x i8], [2 x i8]* %5, i32 0, i32 1
  %9 = zext i1 %4 to i8
  store i8 %9, i8* %8, align 1
  %10 = call i8* @malloc(i64 2)
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %10, i8* %6, i64 2, i1 false)
  call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([14 x i8], [14 x i8]* @cstr.61727261793C6931207820323E00, i32 0, i32 0))
  %11 = bitcast i8* %10 to i1*
  %12 = getelementptr i1, i1* %11, i32 0
  %13 = load i1, i1* %12, align 1
  call void @__quantum__rt__bool_record_output(i1 %13, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @cstr.5B305D00, i32 0, i32 0))
  %14 = getelementptr i1, i1* %11, i32 1
  %15 = load i1, i1* %14, align 1
  call void @__quantum__rt__bool_record_output(i1 %15, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @cstr.5B315D00, i32 0, i32 0))
  ret void
}

define void @__nvqpp__mlirgen__kernel..0x1089303b0.run.entry() {
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #3

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__rt__bool_record_output(i1 zeroext, i8*)

declare void @__quantum__rt__array_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22b0\22]],[1,[1,\22b1\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { nocallback nofree nounwind willreturn memory(argmem: readwrite) }
attributes #2 = { "entry_point" "output_labeling_schema"="schema_id" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #3 = { "irreversible" }

!llvm.module.flags = !{!0}

!0 = !{i32 2, !"Debug Info Version", i32 3}
