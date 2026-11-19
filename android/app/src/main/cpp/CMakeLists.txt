cmake_minimum_required(VERSION 3.31.6)

project(lola_reference_llama LANGUAGES C CXX)

include(FetchContent)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

# Reference backend only: portable CPU ARM64, no GPU/NPU assumptions and no
# architecture-specific global -march flag. ACE remains independent of llama.cpp.
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_COMMON OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TOOLS OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_SERVER OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_APP OFF CACHE BOOL "" FORCE)
set(LLAMA_OPENSSL OFF CACHE BOOL "" FORCE)

set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(GGML_LLAMAFILE OFF CACHE BOOL "" FORCE)
set(GGML_OPENMP OFF CACHE BOOL "" FORCE)
set(GGML_BACKEND_DL OFF CACHE BOOL "" FORCE)
set(GGML_CPU_KLEIDIAI OFF CACHE BOOL "" FORCE)

FetchContent_Declare(
    llama_cpp
    GIT_REPOSITORY https://github.com/ggml-org/llama.cpp.git
    GIT_TAG 8642541b255bfb5dc39cd351a178be09013a55d6
    GIT_SHALLOW FALSE
)
FetchContent_MakeAvailable(llama_cpp)

add_library(lola-reference-llama SHARED lola_reference.cpp)

target_include_directories(lola-reference-llama PRIVATE
    ${llama_cpp_SOURCE_DIR}/include
    ${llama_cpp_SOURCE_DIR}/ggml/include
)

target_compile_options(lola-reference-llama PRIVATE
    -Wall
    -Wextra
    -Wpedantic
)

target_link_libraries(lola-reference-llama PRIVATE
    llama
    android
    log
)
