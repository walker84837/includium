#ifndef C_PREPROCESSOR_H
#define C_PREPROCESSOR_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct CPreprocessor CPreprocessor;

CPreprocessor* c_preprocessor_new(void);

void c_preprocessor_free(CPreprocessor* pp);

char* c_preprocessor_process(CPreprocessor* pp, const char* input);

void c_preprocessor_free_result(char* result);

#ifdef __cplusplus
}
#endif

#endif // C_PREPROCESSOR_H