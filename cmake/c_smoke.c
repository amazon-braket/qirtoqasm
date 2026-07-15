/* SPDX-License-Identifier: Apache-2.0
 *
 * Pure-C smoke test for the qirtoqasm C ABI. Built when
 * `-DQIRTOQASM_BUILD_C_TESTS=ON`. Compiled with `-std=c11
 * -Wall -Wextra -Werror -Wpedantic`.
 *
 * Exercises: translate() with NULL options, translate() with a
 * populated `qirtoqasm_options_t`, error reporting on invalid QIR,
 * qirtoqasm_free_string(NULL), qirtoqasm_version(). Returns 0 on
 * success; any failed check aborts the process.
 */

#include <qirtoqasm/qirtoqasm.h>

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *const kBellQir =
    "%Qubit = type opaque\n"
    "%Result = type opaque\n"
    "define void @main() #0 {\n"
    "  call void @__quantum__qis__h__body(%Qubit* null)\n"
    "  call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))\n"
    "  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)\n"
    "  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))\n"
    "  ret void\n"
    "}\n"
    "declare void @__quantum__qis__h__body(%Qubit*)\n"
    "declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)\n"
    "declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1\n"
    "attributes #0 = { \"entry_point\" \"qir_profiles\"=\"base_profile\" }\n"
    "attributes #1 = { \"irreversible\" }\n";

static int contains(const char *haystack, const char *needle) {
    return strstr(haystack, needle) != NULL;
}

static void test_default_options(void) {
    char *out = NULL;
    char *err = NULL;
    const int rc = qirtoqasm_translate(kBellQir, NULL, &out, &err);
    assert(rc == QIRTOQASM_OK);
    assert(err == NULL);
    assert(out != NULL);
    assert(contains(out, "OPENQASM 3.0;"));
    assert(contains(out, "qubit[2] q;"));
    assert(contains(out, "cnot q[0], q[1];"));
    assert(contains(out, "// generated-by: {\"name\":\"qirtoqasm\","));
    assert(!contains(out, "\"producer\""));
    qirtoqasm_free_string(out);
}

static void test_options_with_producer(void) {
    qirtoqasm_options_t opts;
    qirtoqasm_options_init(&opts);
    assert(opts.struct_version == QIRTOQASM_OPTIONS_VERSION);
    assert(opts.struct_size == sizeof(qirtoqasm_options_t));
    assert(opts.producer == NULL);
    opts.producer = "mylib 0.1.2";

    char *out = NULL;
    char *err = NULL;
    const int rc = qirtoqasm_translate(kBellQir, &opts, &out, &err);
    assert(rc == QIRTOQASM_OK);
    assert(err == NULL);
    assert(contains(out, "\"producer\":\"mylib 0.1.2\""));
    qirtoqasm_free_string(out);
}

static void test_options_with_empty_producer_omits_field(void) {
    qirtoqasm_options_t opts;
    qirtoqasm_options_init(&opts);
    opts.producer = "";
    char *out = NULL;
    char *err = NULL;
    const int rc = qirtoqasm_translate(kBellQir, &opts, &out, &err);
    assert(rc == QIRTOQASM_OK);
    assert(!contains(out, "\"producer\""));
    qirtoqasm_free_string(out);
}

static void test_error_on_invalid_qir(void) {
    char *out = NULL;
    char *err = NULL;
    const int rc = qirtoqasm_translate("not valid qir", NULL, &out, &err);
    assert(rc != QIRTOQASM_OK);
    assert(out == NULL);
    assert(err != NULL);
    assert(strlen(err) > 0);
    qirtoqasm_free_string(err);
}

static void test_uninit_options_rejected(void) {
    /* Forgot to call qirtoqasm_options_init; struct_size==0 must be
     * rejected with a diagnostic rather than silently using defaults. */
    qirtoqasm_options_t opts;
    memset(&opts, 0, sizeof opts);
    char *out = NULL;
    char *err = NULL;
    const int rc = qirtoqasm_translate(kBellQir, &opts, &out, &err);
    assert(rc != QIRTOQASM_OK);
    assert(out == NULL);
    assert(err != NULL);
    assert(contains(err, "struct_size"));
    qirtoqasm_free_string(err);
}

static void test_free_string_tolerates_null(void) {
    qirtoqasm_free_string(NULL);
}

static void test_version_is_non_empty(void) {
    char *v = qirtoqasm_version();
    assert(v != NULL);
    assert(strlen(v) > 0);
    qirtoqasm_free_string(v);
}

int main(void) {
    test_default_options();
    test_options_with_producer();
    test_options_with_empty_producer_omits_field();
    test_error_on_invalid_qir();
    test_uninit_options_rejected();
    test_free_string_tolerates_null();
    test_version_is_non_empty();

    char *v = qirtoqasm_version();
    printf("qirtoqasm C smoke OK (version %s)\n", v);
    qirtoqasm_free_string(v);
    return 0;
}
