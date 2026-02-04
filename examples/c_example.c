#include <stdio.h>
#include <stdlib.h>
#include "../include/c_preprocessor.h"

int main() {
    // Create preprocessor with default config (null = defaults)
    PreprocessorDriver* pp = includium_new(NULL);
    if (!pp) {
        fprintf(stderr, "Failed to create preprocessor\n");
        return 1;
    }

    // Input code to preprocess
    const char* input = "#define PI 3.14\n"
                        "#define ADD(a, b) ((a)+(b))\n"
                        "float x = PI;\n"
                        "int y = ADD(1, 2);\n";

    // Process
    char* result = includium_process(pp, input);
    if (result) {
        printf("Preprocessed output:\n%s\n", result);
        includium_free_result(result);
    } else {
        fprintf(stderr, "Preprocessing failed\n");
    }

    // Clean up
    includium_free(pp);
    return 0;
}