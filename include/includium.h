#ifndef INCLUDIUM_H
#define INCLUDIUM_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Each PreprocessorDriver instance is NOT thread-safe and different driver
// instances can be used safely in parallel threads
typedef struct PreprocessorDriver PreprocessorDriver;

// Configuration structure for the preprocessor
typedef struct includium_config {
  /// Target OS: 0=Linux, 1=Windows, 2=MacOS
  int target;
  /// Compiler: 0=GCC, 1=Clang, 2=MSVC
  int compiler;
  /// Recursion limit
  size_t recursion_limit;
  /// Warning handler callback (optional, can be null)
  void (*warning_handler)(const char *msg);
} includium_config_t;

PreprocessorDriver *includium_new(const includium_config_t *config);

void includium_free(PreprocessorDriver *pp);

char *includium_process(PreprocessorDriver *pp, const char *input);

void includium_free_result(char *result);

const char *includium_last_error(void);

#ifdef __cplusplus
}
#endif

#endif // INCLUDIUM_H
