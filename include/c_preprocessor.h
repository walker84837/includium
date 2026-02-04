#ifndef C_PREPROCESSOR_H
#define C_PREPROCESSOR_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PreprocessorDriver PreprocessorDriver;

// Configuration structure for the preprocessor
struct includium_config {
    /// Target OS: 0=Linux, 1=Windows, 2=MacOS
    int target;
    /// Compiler: 0=GCC, 1=Clang, 2=MSVC
    int compiler;
    /// Recursion limit
    size_t recursion_limit;
    /// Warning handler callback (optional, can be null)
    void (*warning_handler)(const char* msg);
};

PreprocessorDriver* includium_new(const struct includium_config* config);

void includium_free(PreprocessorDriver* pp);

char* includium_process(PreprocessorDriver* pp, const char* input);

void includium_free_result(char* result);

#ifdef __cplusplus
}
#endif

#endif // C_PREPROCESSOR_H