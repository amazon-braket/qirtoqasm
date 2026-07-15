// SPDX-License-Identifier: Apache-2.0
//
// C++ smoke test for the qirtoqasm C++ header. Built when
// `-DQIRTOQASM_BUILD_CPP_TESTS=ON`. Compiled with C++20 and
// `-Wall -Wextra -Werror -Wpedantic`.
//
// Exercises: translate() with defaults, translate() with Options
// populated via C++20 designated initializers, TranslationError on
// invalid input, version(). Returns 0 on success; any failed assertion
// aborts the process.

#include <qirtoqasm/qirtoqasm.hpp>

#include <cassert>
#include <iostream>
#include <string>
#include <string_view>

namespace {

constexpr std::string_view kBellQir = R"(
%Qubit = type opaque
%Result = type opaque
define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
attributes #0 = { "entry_point" "qir_profiles"="base_profile" }
attributes #1 = { "irreversible" }
)";

bool contains(std::string_view haystack, std::string_view needle) {
    return haystack.find(needle) != std::string_view::npos;
}

void test_default_options_translate_bell() {
    const std::string oq3 = qirtoqasm::translate(kBellQir);
    assert(contains(oq3, "OPENQASM 3.0;"));
    assert(contains(oq3, "qubit[2] q;"));
    assert(contains(oq3, "bit[2] c;"));
    assert(contains(oq3, "h q[0];"));
    assert(contains(oq3, "cnot q[0], q[1];"));
    assert(contains(oq3, "// generated-by: {\"name\":\"qirtoqasm\","));
    assert(!contains(oq3, "\"producer\""));
}

void test_options_with_producer_surfaces_in_generated_by_line() {
    const std::string oq3 = qirtoqasm::translate(
        kBellQir, qirtoqasm::Options{.producer = "mylib 0.1.2"});
    assert(contains(oq3, "\"producer\":\"mylib 0.1.2\""));
}

void test_options_with_empty_producer_omits_field() {
    const std::string oq3 = qirtoqasm::translate(
        kBellQir, qirtoqasm::Options{.producer = ""});
    assert(!contains(oq3, "\"producer\""));
}

void test_translation_error_on_invalid_qir() {
    bool threw = false;
    try {
        (void)qirtoqasm::translate("not valid qir");
    } catch (const qirtoqasm::TranslationError &e) {
        threw = true;
        assert(e.code() != QIRTOQASM_OK);
        assert(std::string_view{e.what()}.size() > 0);
    }
    assert(threw && "expected TranslationError");
}

void test_version_is_non_empty() {
    const std::string v = qirtoqasm::version();
    assert(!v.empty());
}

}  // namespace

int main() {
    test_default_options_translate_bell();
    test_options_with_producer_surfaces_in_generated_by_line();
    test_options_with_empty_producer_omits_field();
    test_translation_error_on_invalid_qir();
    test_version_is_non_empty();
    std::cout << "qirtoqasm C++ smoke OK (version " << qirtoqasm::version() << ")\n";
    return 0;
}
