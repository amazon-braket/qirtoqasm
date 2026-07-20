; ModuleID = 'LLVMDialectModule'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"

%Qubit = type opaque
%Result = type opaque

@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"
@cstr.72303030303100 = private constant [7 x i8] c"r00001\00"
@cstr.72303030303200 = private constant [7 x i8] c"r00002\00"
@cstr.72303030303300 = private constant [7 x i8] c"r00003\00"
@cstr.72303030303400 = private constant [7 x i8] c"r00004\00"
@cstr.61727261793C6931207820353E00 = private constant [14 x i8] c"array<i1 x 5>\00"

declare i8* @malloc(i64)

declare void @free(i8*)

define void @__nvqpp__mlirgen__k..0x101d520d0() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  %1 = alloca [5 x i8], align 1
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__array_record_output(i64 5, i8* getelementptr inbounds ([14 x i8], [14 x i8]* @cstr.61727261793C6931207820353E00, i32 0, i32 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303000, i32 0, i32 0))
  %2 = call i1 @__quantum__qis__read_result__body(%Result* null)
  %3 = bitcast [5 x i8]* %1 to i8*
  %4 = zext i1 %2 to i8
  store i8 %4, i8* %3, align 1
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303100, i32 0, i32 0))
  %5 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %6 = getelementptr [5 x i8], [5 x i8]* %1, i32 0, i32 1
  %7 = zext i1 %5 to i8
  store i8 %7, i8* %6, align 1
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303200, i32 0, i32 0))
  %8 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 2 to %Result*))
  %9 = getelementptr [5 x i8], [5 x i8]* %1, i32 0, i32 2
  %10 = zext i1 %8 to i8
  store i8 %10, i8* %9, align 1
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303300, i32 0, i32 0))
  %11 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 3 to %Result*))
  %12 = getelementptr [5 x i8], [5 x i8]* %1, i32 0, i32 3
  %13 = zext i1 %11 to i8
  store i8 %13, i8* %12, align 1
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303400, i32 0, i32 0))
  %14 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 4 to %Result*))
  %15 = getelementptr [5 x i8], [5 x i8]* %1, i32 0, i32 4
  %16 = zext i1 %14 to i8
  store i8 %16, i8* %15, align 1
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__rt__array_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22r00000\22]],[1,[1,\22r00001\22]],[2,[2,\22r00002\22]],[3,[3,\22r00003\22]],[4,[4,\22r00004\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="5" "requiredResults"="5" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0}

!0 = !{i32 2, !"Debug Info Version", i32 3}
