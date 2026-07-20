; ModuleID = 'LLVMDialectModule'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"

%Qubit = type opaque
%Result = type opaque

@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"
@cstr.61727261793C6931207820313E00 = private constant [14 x i8] c"array<i1 x 1>\00"

declare i8* @malloc(i64)

declare void @free(i8*)

define void @__nvqpp__mlirgen__k..0x10b4e6150(double %0) #0 {
  call void @__quantum__qis__rx__body(double 5.000000e-01, %Qubit* null)
  %2 = alloca [1 x i8], align 1
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__array_record_output(i64 1, i8* getelementptr inbounds ([14 x i8], [14 x i8]* @cstr.61727261793C6931207820313E00, i32 0, i32 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303000, i32 0, i32 0))
  %3 = call i1 @__quantum__qis__read_result__body(%Result* null)
  %4 = bitcast [1 x i8]* %2 to i8*
  %5 = zext i1 %3 to i8
  store i8 %5, i8* %4, align 1
  ret void
}

declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1

declare void @__quantum__qis__rx__body(double, %Qubit*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__rt__array_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22r00000\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="1" "requiredResults"="1" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0}

!0 = !{i32 2, !"Debug Info Version", i32 3}
