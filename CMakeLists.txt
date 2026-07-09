cmake_minimum_required(VERSION 3.21)
project(qirtoqasm
    VERSION 0.1.0
    DESCRIPTION "QIR to Braket-compatible OpenQASM 3 translator (native library)"
    LANGUAGES C CXX)

# -----------------------------------------------------------------------------
# Options
# -----------------------------------------------------------------------------
option(QIRTOQASM_BUILD_EXAMPLES "Build the CMake-consumer example" OFF)
option(QIRTOQASM_BUILD_CPP_TESTS "Build the C++ smoke test" OFF)
option(QIRTOQASM_BUILD_C_TESTS "Build the pure-C smoke test" OFF)

# -----------------------------------------------------------------------------
# Rust build of qirtoqasm-ffi
# -----------------------------------------------------------------------------
# We drive `cargo build --release -p qirtoqasm-ffi` as a custom target and
# treat the produced libraries as imported prebuilt artifacts. We
# intentionally do NOT use Corrosion (per the project requirements); this
# keeps the CMake side unconstrained by Corrosion's version compatibility
# matrix.

set(CARGO_BUILD_DIR "${CMAKE_BINARY_DIR}/cargo")
set(CARGO_PROFILE_FLAG --release)
set(CARGO_PROFILE release)

if(APPLE)
    set(QIRTOQASM_STATIC_NAME "libqirtoqasm.a")
    set(QIRTOQASM_SHARED_NAME "libqirtoqasm.dylib")
elseif(WIN32)
    # Cargo names cdylibs `<crate>.dll` (no `lib` prefix) with an
    # accompanying `<crate>.dll.lib` import library on MSVC; the
    # staticlib output is `<crate>.lib`.
    set(QIRTOQASM_STATIC_NAME "qirtoqasm.lib")
    set(QIRTOQASM_SHARED_NAME "qirtoqasm.dll")
    set(QIRTOQASM_IMPORTLIB_NAME "qirtoqasm.dll.lib")
else()
    set(QIRTOQASM_STATIC_NAME "libqirtoqasm.a")
    set(QIRTOQASM_SHARED_NAME "libqirtoqasm.so")
endif()

set(QIRTOQASM_CARGO_TARGET_DIR "${CARGO_BUILD_DIR}/${CARGO_PROFILE}")

# On Linux, ask rustc to embed a SONAME in the cdylib so consumers'
# DT_NEEDED records a bare filename instead of the .so path. Without
# this the recorded path contains slashes, which per ld.so bypasses
# RPATH search at runtime.
if(UNIX AND NOT APPLE)
    set(QIRTOQASM_CARGO_ENV
        CARGO_TARGET_DIR=${CARGO_BUILD_DIR}
        RUSTFLAGS=-Clink-arg=-Wl,-soname,libqirtoqasm.so
    )
else()
    set(QIRTOQASM_CARGO_ENV
        CARGO_TARGET_DIR=${CARGO_BUILD_DIR}
    )
endif()

add_custom_command(
    OUTPUT
        "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_STATIC_NAME}"
        "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_SHARED_NAME}"
    COMMAND ${CMAKE_COMMAND} -E env ${QIRTOQASM_CARGO_ENV}
        cargo build ${CARGO_PROFILE_FLAG} -p qirtoqasm-ffi
    WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
    COMMENT "Building qirtoqasm-ffi (cargo ${CARGO_PROFILE})"
    VERBATIM
)

add_custom_target(qirtoqasm_cargo_build ALL
    DEPENDS
        "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_STATIC_NAME}"
        "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_SHARED_NAME}"
)

# -----------------------------------------------------------------------------
# Imported targets
# -----------------------------------------------------------------------------
add_library(qirtoqasm_static STATIC IMPORTED GLOBAL)
add_library(qirtoqasm::qirtoqasm_static ALIAS qirtoqasm_static)
set_target_properties(qirtoqasm_static PROPERTIES
    IMPORTED_LOCATION "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_STATIC_NAME}"
    INTERFACE_INCLUDE_DIRECTORIES "${CMAKE_SOURCE_DIR}/include"
)

add_library(qirtoqasm_shared SHARED IMPORTED GLOBAL)
add_library(qirtoqasm::qirtoqasm ALIAS qirtoqasm_shared)
set_target_properties(qirtoqasm_shared PROPERTIES
    IMPORTED_LOCATION "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_SHARED_NAME}"
    INTERFACE_INCLUDE_DIRECTORIES "${CMAKE_SOURCE_DIR}/include"
)
if(WIN32)
    # A SHARED IMPORTED target on Windows requires an import library
    # alongside the DLL — CMake enforces this at generate time.
    set_target_properties(qirtoqasm_shared PROPERTIES
        IMPORTED_IMPLIB "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_IMPORTLIB_NAME}"
    )
endif()

add_dependencies(qirtoqasm_static qirtoqasm_cargo_build)
add_dependencies(qirtoqasm_shared qirtoqasm_cargo_build)

# -----------------------------------------------------------------------------
# Install rules
# -----------------------------------------------------------------------------
include(GNUInstallDirs)

install(FILES
    "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_STATIC_NAME}"
    "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_SHARED_NAME}"
    DESTINATION "${CMAKE_INSTALL_LIBDIR}")

install(DIRECTORY include/qirtoqasm
    DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}")

include(CMakePackageConfigHelpers)
configure_package_config_file(
    "${CMAKE_SOURCE_DIR}/cmake/qirtoqasm-config.cmake.in"
    "${CMAKE_BINARY_DIR}/cmake/qirtoqasm-config.cmake"
    INSTALL_DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/qirtoqasm"
    PATH_VARS CMAKE_INSTALL_INCLUDEDIR CMAKE_INSTALL_LIBDIR)

write_basic_package_version_file(
    "${CMAKE_BINARY_DIR}/cmake/qirtoqasm-config-version.cmake"
    VERSION "${PROJECT_VERSION}"
    COMPATIBILITY SameMinorVersion)

install(FILES
    "${CMAKE_BINARY_DIR}/cmake/qirtoqasm-config.cmake"
    "${CMAKE_BINARY_DIR}/cmake/qirtoqasm-config-version.cmake"
    DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/qirtoqasm")

if(QIRTOQASM_BUILD_CPP_TESTS)
    add_executable(qirtoqasm_cpp_smoke cmake/cpp_smoke.cpp)
    target_link_libraries(qirtoqasm_cpp_smoke PRIVATE qirtoqasm::qirtoqasm)
    target_compile_features(qirtoqasm_cpp_smoke PRIVATE cxx_std_20)
    # Runtime search: on ELF/Mach-O, embed BUILD_RPATH so the loader
    # finds libqirtoqasm at the cargo output directory. On Windows,
    # copy qirtoqasm.dll next to the executable post-build (the
    # Windows loader searches the executable's own directory).
    set_target_properties(qirtoqasm_cpp_smoke PROPERTIES
        BUILD_RPATH "${QIRTOQASM_CARGO_TARGET_DIR}"
    )
    if(WIN32)
        add_custom_command(TARGET qirtoqasm_cpp_smoke POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_SHARED_NAME}"
                "$<TARGET_FILE_DIR:qirtoqasm_cpp_smoke>"
        )
    endif()
    # Keep asserts active in Release builds - these are test executables.
    if(MSVC)
        target_compile_options(qirtoqasm_cpp_smoke PRIVATE /W4 /WX /permissive- /UNDEBUG)
    else()
        target_compile_options(qirtoqasm_cpp_smoke PRIVATE
            -Wall -Wextra -Werror -Wpedantic -UNDEBUG)
    endif()
endif()

if(QIRTOQASM_BUILD_C_TESTS)
    add_executable(qirtoqasm_c_smoke cmake/c_smoke.c)
    target_link_libraries(qirtoqasm_c_smoke PRIVATE qirtoqasm::qirtoqasm)
    set_target_properties(qirtoqasm_c_smoke PROPERTIES
        C_STANDARD 11
        C_STANDARD_REQUIRED ON
        C_EXTENSIONS OFF
        BUILD_RPATH "${QIRTOQASM_CARGO_TARGET_DIR}")
    if(WIN32)
        add_custom_command(TARGET qirtoqasm_c_smoke POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_SHARED_NAME}"
                "$<TARGET_FILE_DIR:qirtoqasm_c_smoke>"
        )
    endif()
    if(MSVC)
        target_compile_options(qirtoqasm_c_smoke PRIVATE /W4 /WX /UNDEBUG)
    else()
        target_compile_options(qirtoqasm_c_smoke PRIVATE
            -Wall -Wextra -Werror -Wpedantic -UNDEBUG)
    endif()
endif()
