; Controlled-H is not a Braket-native gate; qirtoqasm should refuse with a
; descriptive message pointing at upstream decomposition.
; ModuleID = 'LLVMDialectModule'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"

%Qubit = type opaque
%Result = type opaque
%Array = type opaque

@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"

define void @__nvqpp__mlirgen__k..0x100b00000() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void (i64, i64, i64, i64, i8*, ...) @generalizedInvokeWithRotationsControlsTargets(i64 0, i64 0, i64 1, i64 1, i8* bitcast (void (%Array*, %Qubit*)* @__quantum__qis__h__ctl to i8*), i8* null, i8* inttoptr (i64 1 to i8*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303000, i32 0, i32 0))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__h__ctl(%Array*, %Qubit*)
declare void @generalizedInvokeWithRotationsControlsTargets(i64, i64, i64, i64, i8*, ...)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22r00000\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="1" }
attributes #1 = { "irreversible" }
