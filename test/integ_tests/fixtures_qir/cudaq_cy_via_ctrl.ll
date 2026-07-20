; ModuleID = 'LLVMDialectModule'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"

%Qubit = type opaque
%Result = type opaque
%Array = type opaque

@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"
@cstr.72303030303100 = private constant [7 x i8] c"r00001\00"
@cstr.61727261793C6931207820323E00 = private constant [14 x i8] c"array<i1 x 2>\00"

declare i8* @malloc(i64)

declare void @free(i8*)

define void @__nvqpp__mlirgen__k..0x103c890d0() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void (i64, i64, i64, i64, i8*, ...) @generalizedInvokeWithRotationsControlsTargets(i64 0, i64 0, i64 1, i64 1, i8* bitcast (void (%Array*, %Qubit*)* @__quantum__qis__y__ctl to i8*), i8* null, i8* inttoptr (i64 1 to i8*))
  %1 = alloca [2 x i8], align 1
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([14 x i8], [14 x i8]* @cstr.61727261793C6931207820323E00, i32 0, i32 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303000, i32 0, i32 0))
  %2 = call i1 @__quantum__qis__read_result__body(%Result* null)
  %3 = bitcast [2 x i8]* %1 to i8*
  %4 = zext i1 %2 to i8
  store i8 %4, i8* %3, align 1
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303100, i32 0, i32 0))
  %5 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %6 = getelementptr [2 x i8], [2 x i8]* %1, i32 0, i32 1
  %7 = zext i1 %5 to i8
  store i8 %7, i8* %6, align 1
  ret void
}

declare void @__quantum__qis__y__ctl(%Array*, %Qubit*)

declare void @generalizedInvokeWithRotationsControlsTargets(i64, i64, i64, i64, i8*, ...)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__rt__array_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22r00000\22]],[1,[1,\22r00001\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0}

!0 = !{i32 2, !"Debug Info Version", i32 3}
