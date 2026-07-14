; ModuleID = 'LLVMDialectModule'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"

%Qubit = type opaque
%Result = type opaque

@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"
@cstr.72303030303100 = private constant [7 x i8] c"r00001\00"
@cstr.72303030303200 = private constant [7 x i8] c"r00002\00"
@cstr.61727261793C6931207820333E00 = private constant [14 x i8] c"array<i1 x 3>\00"

declare i8* @malloc(i64)

declare void @free(i8*)

define void @__nvqpp__mlirgen__k..0x102073f90() #0 {
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  %1 = alloca [3 x i8], align 1
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__array_record_output(i64 3, i8* getelementptr inbounds ([14 x i8], [14 x i8]* @cstr.61727261793C6931207820333E00, i32 0, i32 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303000, i32 0, i32 0))
  %2 = call i1 @__quantum__qis__read_result__body(%Result* null)
  %3 = bitcast [3 x i8]* %1 to i8*
  %4 = zext i1 %2 to i8
  store i8 %4, i8* %3, align 1
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303100, i32 0, i32 0))
  %5 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %6 = getelementptr [3 x i8], [3 x i8]* %1, i32 0, i32 1
  %7 = zext i1 %5 to i8
  store i8 %7, i8* %6, align 1
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303200, i32 0, i32 0))
  %8 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 2 to %Result*))
  %9 = getelementptr [3 x i8], [3 x i8]* %1, i32 0, i32 2
  %10 = zext i1 %8 to i8
  store i8 %10, i8* %9, align 1
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__rt__array_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22r00000\22]],[1,[1,\22r00001\22]],[2,[2,\22r00002\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="4" "requiredResults"="3" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0}

!0 = !{i32 2, !"Debug Info Version", i32 3}
