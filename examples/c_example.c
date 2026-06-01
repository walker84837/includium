#include "../include/includium.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv) {
  char *input = NULL;

  if (argc > 1 && argv[1]) {
    FILE *file = fopen(argv[1], "rb");
    if (file) {
      // Get file size
      fseek(file, 0, SEEK_END);
      long size = ftell(file);
      rewind(file);

      // Allocate (size + 1) bytes with calloc so the buffer is NUL-terminated
      // even if fread reads fewer bytes

      // This avoids buffer-overruns and makes `input` safe to use as a C
      // string.
      if (size > 0) {
        input = calloc(1, (size_t)size + 1);
        if (input) {
          fread(input, 1, (size_t)size, file);
        }
      }

      fclose(file);
    }
  } else {
    input = strdup("#define PI 3.14\n"
                   "#define ADD(a, b) ((a)+(b))\n"
                   "float x = PI;\n"
                   "int y = ADD(1, 2);\n");
  }

  // Create preprocessor with default config (null = defaults)
  includium_ctx *pp = includium_new(NULL);
  if (!pp) {
    fprintf(stderr, "Failed to create preprocessor\n");
    return 1;
  }

  // Process the macros
  char *result = includium_process(pp, input);

  if (result) {
    printf("Preprocessed output:\n%s\n", result);
    includium_free_result(result);
  } else {
    fprintf(stderr, "Preprocessing failed\n");
  }

  // Final cleanup
  includium_free(pp);
  free(input);
  return 0;
}
