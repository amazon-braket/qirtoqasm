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
    set(QIRTOQASM_SHARED_SUFFIX ".dylib")
elseif(UNIX)
    set(QIRTOQASM_SHARED_SUFFIX ".so")
elseif(WIN32)
    set(QIRTOQASM_SHARED_SUFFIX ".dll")
else()
    set(QIRTOQASM_SHARED_SUFFIX ".so")
endif()

set(QIRTOQASM_STATIC_NAME "libqirtoqasm.a")
set(QIRTOQASM_SHARED_NAME "libqirtoqasm${QIRTOQASM_SHARED_SUFFIX}")

set(QIRTOQASM_CARGO_TARGET_DIR "${CARGO_BUILD_DIR}/${CARGO_PROFILE}")

add_custom_command(
    OUTPUT
        "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_STATIC_NAME}"
        "${QIRTOQASM_CARGO_TARGET_DIR}/${QIRTOQASM_SHARED_NAME}"
    COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${CARGO_BUILD_DIR}
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
    if(MSVC)
        target_compile_options(qirtoqasm_cpp_smoke PRIVATE /W4 /WX /permissive-)
    else()
        target_compile_options(qirtoqasm_cpp_smoke PRIVATE
            -Wall -Wextra -Werror -Wpedantic)
    endif()
endif()

if(QIRTOQASM_BUILD_C_TESTS)
    add_executable(qirtoqasm_c_smoke cmake/c_smoke.c)
    target_link_libraries(qirtoqasm_c_smoke PRIVATE qirtoqasm::qirtoqasm)
    set_target_properties(qirtoqasm_c_smoke PROPERTIES
        C_STANDARD 11
        C_STANDARD_REQUIRED ON
        C_EXTENSIONS OFF)
    if(MSVC)
        target_compile_options(qirtoqasm_c_smoke PRIVATE /W4 /WX)
    else()
        target_compile_options(qirtoqasm_c_smoke PRIVATE
            -Wall -Wextra -Werror -Wpedantic)
    endif()
endif()
