cmake_minimum_required(VERSION 3.16)
project(apex_match C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Optimization flags for ultra-low latency C/C++ clients
if(MSVC)
    add_compile_options(/O2 /Oi /Ot /GL /arch:AVX2 /W4)
else()
    add_compile_options(-O3 -march=native -mtune=native -Wall -Wextra -Wpedantic -pipe)
endif()

include_directories(
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)

# Custom target to build Rust matching engine library
add_custom_target(cargo_build
    COMMAND cargo build --release
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    COMMENT "Compiling apex-match Rust core engine"
)

# C API Header Interface
add_library(apex_match_interface INTERFACE)
target_include_directories(apex_match_interface INTERFACE
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)

message(STATUS "Configured apex-match CMake build system for C/C++ interop")
