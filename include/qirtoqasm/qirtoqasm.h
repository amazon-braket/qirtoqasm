/* SPDX-License-Identifier: Apache-2.0 */
/* Copyright Amazon.com Inc. or its affiliates. All Rights Reserved. */

/* C ABI for the qirtoqasm translator.
 *
 * This header mirrors `crates/qirtoqasm-ffi/src/lib.rs`.
 *
 * All tunables flow through `qirtoqasm_options_t`. The struct carries
 * its own `struct_version` / `struct_size`; new fields will be
 * appended so the library can size-gate reads against an older
 * caller. Call `qirtoqasm_options_init` before setting fields, or
 * pass `NULL` for all defaults.
 */

#ifndef QIRTOQASM_H
#define QIRTOQASM_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define QIRTOQASM_OK 0
#define QIRTOQASM_ERR_SYNTAX 1
#define QIRTOQASM_ERR_UNSUPPORTED 2
#define QIRTOQASM_ERR_UNSUPPORTED_CFG 3
#define QIRTOQASM_ERR_INTERNAL 4

/* Current ABI version of qirtoqasm_options_t. */
#define QIRTOQASM_OPTIONS_VERSION 1

/* Tunables for qirtoqasm_translate.
 *
 * Fields:
 *  - struct_version: set by qirtoqasm_options_init.
 *  - struct_size:    set by qirtoqasm_options_init.
 *  - producer:       optional upstream producer label surfaced as the
 *                    `"producer"` field in the trailing `// generated-by:`
 *                    comment (e.g. "mylib 0.1.2"). NULL or empty omits.
 */
typedef struct qirtoqasm_options {
    uint32_t struct_version;
    uint32_t struct_size;
    const char *producer;
} qirtoqasm_options_t;

/* Initialize a caller-provided options struct to current defaults.
 * NULL is a no-op. */
void qirtoqasm_options_init(qirtoqasm_options_t *opts);

/* Translate QIR text to Braket-compatible OpenQASM 3.
 *
 * `opts` may be NULL for all defaults.
 *
 * On success:  writes heap-allocated OQ3 NUL-terminated string to *out,
 *              sets *err = NULL, returns QIRTOQASM_OK.
 * On failure:  writes heap-allocated message to *err, sets *out = NULL,
 *              returns a non-zero QIRTOQASM_ERR_* code.
 *
 * Ownership:   caller frees *out / *err via qirtoqasm_free_string.
 * Thread safe: yes (all state in arguments).
 */
int qirtoqasm_translate(const char *qir,
                        const qirtoqasm_options_t *opts,
                        char **out,
                        char **err);

/* Release a string previously returned via *out or *err.
 * A NULL pointer is a no-op. */
void qirtoqasm_free_string(char *s);

/* Return the library version as a heap-allocated NUL-terminated string.
 * Caller owns it; free with qirtoqasm_free_string. */
char *qirtoqasm_version(void);

#ifdef __cplusplus
}
#endif

#endif /* QIRTOQASM_H */
