// SPDX-License-Identifier: Apache-2.0
// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// C++20 convenience wrapper over the qirtoqasm C ABI.
//
// Typical usage:
//
//   #include <qirtoqasm/qirtoqasm.hpp>
//
//   std::string oq3 = qirtoqasm::translate(qir_text);
//
//   std::string oq3 = qirtoqasm::translate(qir_text,
//       qirtoqasm::Options{ .producer = "mylib 0.1.2" });
//
// Install via CMake `find_package(qirtoqasm REQUIRED)` and link
// `qirtoqasm::qirtoqasm`.

#pragma once

#include <qirtoqasm/qirtoqasm.h>

#include <stdexcept>
#include <string>
#include <string_view>

namespace qirtoqasm {

/// Tunables for [`translate`]. Adding a field is non-breaking:
/// callers use designated initializers that leave unlisted fields at
/// their defaults.
struct Options {
    /// Upstream producer label (e.g. ``"mylib 0.1.2"``) surfaced as
    /// the ``"producer"`` field in the trailing ``// generated-by:``
    /// comment. Empty omits the field.
    std::string producer = {};
};

/// Exception raised by [`translate`] on any non-zero return code.
class TranslationError : public std::runtime_error {
public:
    TranslationError(int code, std::string message)
        : std::runtime_error(std::move(message)), code_(code) {}

    /// One of the ``QIRTOQASM_ERR_*`` constants.
    [[nodiscard]] int code() const noexcept { return code_; }

private:
    int code_;
};

/// Translate QIR text to a Braket-compatible OpenQASM 3 string.
/// Throws [`TranslationError`] on any failure.
inline std::string translate(std::string_view qir, const Options &options = {}) {
    std::string qir_buffer(qir);
    ::qirtoqasm_options_t c_opts;
    ::qirtoqasm_options_init(&c_opts);
    c_opts.producer = options.producer.empty() ? nullptr : options.producer.c_str();
    char *out = nullptr;
    char *err = nullptr;
    const int rc = ::qirtoqasm_translate(qir_buffer.c_str(), &c_opts, &out, &err);
    if (rc != QIRTOQASM_OK) {
        std::string message = err ? err : "unknown error";
        ::qirtoqasm_free_string(err);
        ::qirtoqasm_free_string(out);
        throw TranslationError(rc, std::move(message));
    }
    std::string result = out ? std::string(out) : std::string();
    ::qirtoqasm_free_string(out);
    return result;
}

/// Return the library version.
inline std::string version() {
    char *p = ::qirtoqasm_version();
    std::string v = p ? std::string(p) : std::string();
    ::qirtoqasm_free_string(p);
    return v;
}

}  // namespace qirtoqasm
