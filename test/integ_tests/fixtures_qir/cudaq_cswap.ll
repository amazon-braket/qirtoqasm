; Hand-crafted fixture mirroring the shape CUDA-Q emits for
; swap<cudaq::ctrl>(ctrl, a, b) after its adaptive-profile lowering.
; numRotations=0, adjoint=0, numControls=1, numTargets=2, inner=@__quantum__qis__swap__ctl
; followed by one control pointer and two target pointers.
; ModuleID = 'LLVMDialectModule'
source_filename = "LLVMDialectModule"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"

%Qubit = type opaque
%Result = type opaque
%Array = type opaque

@cstr.72303030303000 = private constant [7 x i8] c"r00000\00"
@cstr.72303030303100 = private constant [7 x i8] c"r00001\00"
@cstr.72303030303200 = private constant [7 x i8] c"r00002\00"
@cstr.61727261793C6931207820333E00 = private constant [14 x i8] c"array<i1 x 3>\00"

define void @__nvqpp__mlirgen__k..0x100a00000() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void (i64, i64, i64, i64, i8*, ...) @generalizedInvokeWithRotationsControlsTargets(i64 0, i64 0, i64 1, i64 2, i8* bitcast (void (%Array*, %Qubit*, %Qubit*)* @__quantum__qis__swap__ctl to i8*), i8* null, i8* inttoptr (i64 1 to i8*), i8* inttoptr (i64 2 to i8*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303000, i32 0, i32 0))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303100, i32 0, i32 0))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([7 x i8], [7 x i8]* @cstr.72303030303200, i32 0, i32 0))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__swap__ctl(%Array*, %Qubit*, %Qubit*)
declare void @generalizedInvokeWithRotationsControlsTargets(i64, i64, i64, i64, i8*, ...)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema"="schema_id" "output_names"="[[[0,[0,\22r00000\22]],[1,[1,\22r00001\22]],[2,[2,\22r00002\22]]]]" "qir_profiles"="adaptive_profile" "requiredQubits"="3" "requiredResults"="3" }
attributes #1 = { "irreversible" }
