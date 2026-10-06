# =====================================================================
# CMakeLists.txt — camadas C/C++ do pipeline OnyxChat (K4 + K9)
# ---------------------------------------------------------------------
# Constrói a biblioteca estática `onyx_crypto` (transposição + K9) e os
# testes CTest correspondentes.
#
# Uso:
#   cmake -S crypto/c_cpp -B crypto/c_cpp/build
#   cmake --build crypto/c_cpp/build
#   ctest --test-dir crypto/c_cpp/build --output-on-failure
#
# Cobertura gcov (para gcovr):
#   cmake -S crypto/c_cpp -B build -DONYX_COVERAGE=ON
# =====================================================================

cmake_minimum_required(VERSION 3.16)
project(onyx_crypto_c LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Sem NDEBUG nos testes: os CHECKs são nossos, mas queremos warnings.
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Debug)
endif()

option(ONYX_COVERAGE "Ativar instrumentação gcov" OFF)
if(ONYX_COVERAGE)
    add_compile_options(--coverage -O0 -g)
    add_link_options(--coverage)
endif()

# ---------------------------------------------------------------------
# libsodium: usa o vendor estático incluído no repositório; se não
# existir, cai para o libsodium do sistema (libsodium-dev).
# ---------------------------------------------------------------------
set(ONYX_SODIUM_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/vendor/libsodium")
if(EXISTS "${ONYX_SODIUM_ROOT}/lib/libsodium.a")
    set(SODIUM_LIB "${ONYX_SODIUM_ROOT}/lib/libsodium.a")
    set(SODIUM_INCLUDE "${ONYX_SODIUM_ROOT}/include")
    message(STATUS "libsodium: vendor estático em ${ONYX_SODIUM_ROOT}")
else()
    find_path(SODIUM_INCLUDE sodium.h)
    find_library(SODIUM_LIB sodium)
    if(NOT SODIUM_INCLUDE OR NOT SODIUM_LIB)
        message(FATAL_ERROR "libsodium não encontrado (instale libsodium-dev)")
    endif()
    message(STATUS "libsodium: sistema → ${SODIUM_LIB}")
endif()

# ---------------------------------------------------------------------
# Biblioteca estática partilhada pelo CTest e pelo build.rs do daemon
# ---------------------------------------------------------------------
add_library(onyx_crypto STATIC transposition.cpp k9_chacha.c)
target_include_directories(onyx_crypto PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
target_include_directories(onyx_crypto PRIVATE "${SODIUM_INCLUDE}")
target_link_libraries(onyx_crypto PUBLIC "${SODIUM_LIB}")

# Warnings rigorosos (qualidade de código alinhada com o clippy)
target_compile_options(onyx_crypto PRIVATE
    $<$<COMPILE_LANGUAGE:C>:-Wall -Wextra -Wpedantic>
    $<$<COMPILE_LANGUAGE:CXX>:-Wall -Wextra -Wpedantic>
)

# ---------------------------------------------------------------------
# Testes CTest
# ---------------------------------------------------------------------
enable_testing()

add_executable(test_transposition tests/test_transposition.cpp)
target_link_libraries(test_transposition PRIVATE onyx_crypto)
add_test(NAME transposition COMMAND test_transposition)

add_executable(test_k9 tests/test_k9.c)
target_link_libraries(test_k9 PRIVATE onyx_crypto)
add_test(NAME k9 COMMAND test_k9)

# ---------------------------------------------------------------------
# Vetores oficiais (tests/vectors/camadas.json — gerador Rust)
# Verifica K4 (C++) e K9 (C) contra os outputs documentados (§25).
# ---------------------------------------------------------------------
add_executable(test_vectors tests/test_vectors.c)
target_link_libraries(test_vectors PRIVATE onyx_crypto)
target_compile_definitions(test_vectors PRIVATE
    VETORES_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../../tests/vectors")
add_test(NAME vectors COMMAND test_vectors)
