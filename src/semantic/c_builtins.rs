//! Built-in libc/libm C symbol tables for semantic analysis.
//!
//! Kept out of `analyzer.rs` so C-interop work does not contend with
//! statement analysis on the same file.

use std::collections::HashMap;

use crate::c::{CSymbol, CSymbolTable};
use crate::parser::function::Parameter;

/// Default `.cfc` tables for `libc` and `libm`.
pub fn builtin_cfc_tables() -> HashMap<String, CSymbolTable> {
        let mut builtin_symbols = HashMap::new();

        // libc标准函数
        let mut libc_symbols = HashMap::new();

        // void exit(int status)
        libc_symbols.insert("exit".to_string(), CSymbol {
            name: "exit".to_string(),
            parameters: vec![Parameter {
                name: "status".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int puts(const char *s)
        libc_symbols.insert("puts".to_string(), CSymbol {
            name: "puts".to_string(),
            parameters: vec![Parameter {
                name: "s".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void *malloc(size_t size)
        libc_symbols.insert("malloc".to_string(), CSymbol {
            name: "malloc".to_string(),
            parameters: vec![Parameter {
                name: "size".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // void free(void *ptr)
        libc_symbols.insert("free".to_string(), CSymbol {
            name: "free".to_string(),
            parameters: vec![Parameter {
                name: "ptr".to_string(),
                param_type: "object".to_string(), // pointer as int
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int printf(const char *format, ...)
        libc_symbols.insert("printf".to_string(), CSymbol {
            name: "printf".to_string(),
            parameters: vec![Parameter {
                name: "format".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int fprintf(FILE *stream, const char *format, ...)
        libc_symbols.insert("fprintf".to_string(), CSymbol {
            name: "fprintf".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "format".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int fputs(const char *s, int stream)
        libc_symbols.insert("fputs".to_string(), CSymbol {
            name: "fputs".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int putchar(int c)
        libc_symbols.insert("putchar".to_string(), CSymbol {
            name: "putchar".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fwrite(const void *ptr, int size, int n, int stream)
        libc_symbols.insert("fwrite".to_string(), CSymbol {
            name: "fwrite".to_string(),
            parameters: vec![
                Parameter {
                    name: "ptr".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int strlen(const char *s)
        libc_symbols.insert("strlen".to_string(), CSymbol {
            name: "strlen".to_string(),
            parameters: vec![Parameter {
                name: "s".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int strcmp(const char *s1, const char *s2)
        libc_symbols.insert("strcmp".to_string(), CSymbol {
            name: "strcmp".to_string(),
            parameters: vec![
                Parameter {
                    name: "s1".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "s2".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *strncpy(char *dest, const char *src, int n)
        libc_symbols.insert("strncpy".to_string(), CSymbol {
            name: "strncpy".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // int strncmp(const char *s1, const char *s2, int n)
        libc_symbols.insert("strncmp".to_string(), CSymbol {
            name: "strncmp".to_string(),
            parameters: vec![
                Parameter {
                    name: "s1".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "s2".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *strchr(const char *s, int c)
        libc_symbols.insert("strchr".to_string(), CSymbol {
            name: "strchr".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "c".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // char *strstr(const char *haystack, const char *needle)
        libc_symbols.insert("strstr".to_string(), CSymbol {
            name: "strstr".to_string(),
            parameters: vec![
                Parameter {
                    name: "haystack".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "needle".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // void *memcpy(void *dest, const void *src, int n)
        libc_symbols.insert("memcpy".to_string(), CSymbol {
            name: "memcpy".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "object".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // void *memset(void *s, int c, int n)
        libc_symbols.insert("memset".to_string(), CSymbol {
            name: "memset".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "object".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "c".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // int memcmp(const void *ptr1, const void *ptr2, int n)
        libc_symbols.insert("memcmp".to_string(), CSymbol {
            name: "memcmp".to_string(),
            parameters: vec![
                Parameter {
                    name: "ptr1".to_string(),
                    param_type: "object".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "ptr2".to_string(),
                    param_type: "object".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void abort(void)
        libc_symbols.insert("abort".to_string(), CSymbol {
            name: "abort".to_string(),
            parameters: vec![],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int atoi(const char *nptr)
        libc_symbols.insert("atoi".to_string(), CSymbol {
            name: "atoi".to_string(),
            parameters: vec![Parameter {
                name: "nptr".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // double atof(const char *nptr)
        libc_symbols.insert("atof".to_string(), CSymbol {
            name: "atof".to_string(),
            parameters: vec![Parameter {
                name: "nptr".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // long atol(const char *nptr)
        libc_symbols.insert("atol".to_string(), CSymbol {
            name: "atol".to_string(),
            parameters: vec![Parameter {
                name: "nptr".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // long long atoll(const char *nptr)
        libc_symbols.insert("atoll".to_string(), CSymbol {
            name: "atoll".to_string(),
            parameters: vec![Parameter {
                name: "nptr".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *getenv(const char *name)
        libc_symbols.insert("getenv".to_string(), CSymbol {
            name: "getenv".to_string(),
            parameters: vec![Parameter {
                name: "name".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // int setenv(const char *name, const char *value, int overwrite)
        libc_symbols.insert("setenv".to_string(), CSymbol {
            name: "setenv".to_string(),
            parameters: vec![
                Parameter {
                    name: "name".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "overwrite".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int system(const char *command)
        libc_symbols.insert("system".to_string(), CSymbol {
            name: "system".to_string(),
            parameters: vec![Parameter {
                name: "command".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int rand(void)
        libc_symbols.insert("rand".to_string(), CSymbol {
            name: "rand".to_string(),
            parameters: vec![],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void srand(unsigned int seed)
        libc_symbols.insert("srand".to_string(), CSymbol {
            name: "srand".to_string(),
            parameters: vec![Parameter {
                name: "seed".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int time(int *tloc)
        libc_symbols.insert("time".to_string(), CSymbol {
            name: "time".to_string(),
            parameters: vec![Parameter {
                name: "tloc".to_string(),
                param_type: "object".to_string(), // pointer as int
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void perror(const char *s)
        libc_symbols.insert("perror".to_string(), CSymbol {
            name: "perror".to_string(),
            parameters: vec![Parameter {
                name: "s".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // char *strcat(char *dest, const char *src)
        libc_symbols.insert("strcat".to_string(), CSymbol {
            name: "strcat".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // char *strncat(char *dest, const char *src, int n)
        libc_symbols.insert("strncat".to_string(), CSymbol {
            name: "strncat".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // char *strcpy(char *dest, const char *src)
        libc_symbols.insert("strcpy".to_string(), CSymbol {
            name: "strcpy".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int isalpha(int c)
        libc_symbols.insert("isalpha".to_string(), CSymbol {
            name: "isalpha".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int isdigit(int c)
        libc_symbols.insert("isdigit".to_string(), CSymbol {
            name: "isdigit".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int isalnum(int c)
        libc_symbols.insert("isalnum".to_string(), CSymbol {
            name: "isalnum".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int isspace(int c)
        libc_symbols.insert("isspace".to_string(), CSymbol {
            name: "isspace".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int islower(int c)
        libc_symbols.insert("islower".to_string(), CSymbol {
            name: "islower".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int isupper(int c)
        libc_symbols.insert("isupper".to_string(), CSymbol {
            name: "isupper".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int tolower(int c)
        libc_symbols.insert("tolower".to_string(), CSymbol {
            name: "tolower".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int toupper(int c)
        libc_symbols.insert("toupper".to_string(), CSymbol {
            name: "toupper".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // FILE *fopen(const char *filename, const char *mode)
        libc_symbols.insert("fopen".to_string(), CSymbol {
            name: "fopen".to_string(),
            parameters: vec![
                Parameter {
                    name: "filename".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "mode".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(), // FILE* as int
            is_variadic: false,
        });

        // int fclose(FILE *stream)
        libc_symbols.insert("fclose".to_string(), CSymbol {
            name: "fclose".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(), // FILE* as int
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fseek(FILE *stream, long offset, int whence)
        libc_symbols.insert("fseek".to_string(), CSymbol {
            name: "fseek".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "whence".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // long ftell(FILE *stream)
        libc_symbols.insert("ftell".to_string(), CSymbol {
            name: "ftell".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void rewind(FILE *stream)
        libc_symbols.insert("rewind".to_string(), CSymbol {
            name: "rewind".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int fflush(FILE *stream)
        libc_symbols.insert("fflush".to_string(), CSymbol {
            name: "fflush".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int feof(FILE *stream)
        libc_symbols.insert("feof".to_string(), CSymbol {
            name: "feof".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int ferror(FILE *stream)
        libc_symbols.insert("ferror".to_string(), CSymbol {
            name: "ferror".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void clearerr(FILE *stream)
        libc_symbols.insert("clearerr".to_string(), CSymbol {
            name: "clearerr".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int remove(const char *filename)
        libc_symbols.insert("remove".to_string(), CSymbol {
            name: "remove".to_string(),
            parameters: vec![Parameter {
                name: "filename".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int rename(const char *oldname, const char *newname)
        libc_symbols.insert("rename".to_string(), CSymbol {
            name: "rename".to_string(),
            parameters: vec![
                Parameter {
                    name: "oldname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "newname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // FILE *tmpfile(void)
        libc_symbols.insert("tmpfile".to_string(), CSymbol {
            name: "tmpfile".to_string(),
            parameters: vec![],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // char *tmpnam(char *s)
        libc_symbols.insert("tmpnam".to_string(), CSymbol {
            name: "tmpnam".to_string(),
            parameters: vec![Parameter {
                name: "s".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int setvbuf(FILE *stream, char *buf, int mode, int size)
        libc_symbols.insert("setvbuf".to_string(), CSymbol {
            name: "setvbuf".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "buf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "mode".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void setbuf(FILE *stream, char *buf)
        libc_symbols.insert("setbuf".to_string(), CSymbol {
            name: "setbuf".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "buf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int getchar(void)
        libc_symbols.insert("getchar".to_string(), CSymbol {
            name: "getchar".to_string(),
            parameters: vec![],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int putchar(int c)
        libc_symbols.insert("putchar".to_string(), CSymbol {
            name: "putchar".to_string(),
            parameters: vec![Parameter {
                name: "c".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int puts(const char *s)
        libc_symbols.insert("puts".to_string(), CSymbol {
            name: "puts".to_string(),
            parameters: vec![Parameter {
                name: "s".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int gets(char *s) - Deprecated but commonly used
        libc_symbols.insert("gets".to_string(), CSymbol {
            name: "gets".to_string(),
            parameters: vec![Parameter {
                name: "s".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *fgets(char *s, int size, FILE *stream)
        libc_symbols.insert("fgets".to_string(), CSymbol {
            name: "fgets".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int fputs(const char *s, FILE *stream)
        libc_symbols.insert("fputs".to_string(), CSymbol {
            name: "fputs".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // double sin(double x)
        libc_symbols.insert("sin".to_string(), CSymbol {
            name: "sin".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double cos(double x)
        libc_symbols.insert("cos".to_string(), CSymbol {
            name: "cos".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double tan(double x)
        libc_symbols.insert("tan".to_string(), CSymbol {
            name: "tan".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double asin(double x)
        libc_symbols.insert("asin".to_string(), CSymbol {
            name: "asin".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double acos(double x)
        libc_symbols.insert("acos".to_string(), CSymbol {
            name: "acos".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double atan(double x)
        libc_symbols.insert("atan".to_string(), CSymbol {
            name: "atan".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double atan2(double y, double x)
        libc_symbols.insert("atan2".to_string(), CSymbol {
            name: "atan2".to_string(),
            parameters: vec![
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double sinh(double x)
        libc_symbols.insert("sinh".to_string(), CSymbol {
            name: "sinh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double cosh(double x)
        libc_symbols.insert("cosh".to_string(), CSymbol {
            name: "cosh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double tanh(double x)
        libc_symbols.insert("tanh".to_string(), CSymbol {
            name: "tanh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double exp(double x)
        libc_symbols.insert("exp".to_string(), CSymbol {
            name: "exp".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double log(double x)
        libc_symbols.insert("log".to_string(), CSymbol {
            name: "log".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double log10(double x)
        libc_symbols.insert("log10".to_string(), CSymbol {
            name: "log10".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double pow(double x, double y)
        libc_symbols.insert("pow".to_string(), CSymbol {
            name: "pow".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double sqrt(double x)
        libc_symbols.insert("sqrt".to_string(), CSymbol {
            name: "sqrt".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double ceil(double x)
        libc_symbols.insert("ceil".to_string(), CSymbol {
            name: "ceil".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double floor(double x)
        libc_symbols.insert("floor".to_string(), CSymbol {
            name: "floor".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double fabs(double x)
        libc_symbols.insert("fabs".to_string(), CSymbol {
            name: "fabs".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double fmod(double x, double y)
        libc_symbols.insert("fmod".to_string(), CSymbol {
            name: "fmod".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // void *malloc(int size)
        libc_symbols.insert("malloc".to_string(), CSymbol {
            name: "malloc".to_string(),
            parameters: vec![Parameter {
                name: "size".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // void free(void *ptr)
        libc_symbols.insert("free".to_string(), CSymbol {
            name: "free".to_string(),
            parameters: vec![Parameter {
                name: "ptr".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // void *calloc(int nmemb, int size)
        libc_symbols.insert("calloc".to_string(), CSymbol {
            name: "calloc".to_string(),
            parameters: vec![
                Parameter {
                    name: "nmemb".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // void *realloc(void *ptr, int size)
        libc_symbols.insert("realloc".to_string(), CSymbol {
            name: "realloc".to_string(),
            parameters: vec![
                Parameter {
                    name: "ptr".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int qsort comparison function type - not directly callable
        // void qsort(void *base, int nmemb, int size, int (*compar)(const void *, const void *))
        libc_symbols.insert("qsort".to_string(), CSymbol {
            name: "qsort".to_string(),
            parameters: vec![
                Parameter {
                    name: "base".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "nmemb".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "compar".to_string(),
                    param_type: "object".to_string(), // function pointer as object
                    is_variadic: false,
                }
            ],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // void *bsearch(const void *key, const void *base, int nmemb, int size, int (*compar)(const void *, const void *))
        libc_symbols.insert("bsearch".to_string(), CSymbol {
            name: "bsearch".to_string(),
            parameters: vec![
                Parameter {
                    name: "key".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "base".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "nmemb".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "compar".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int abs(int x)
        libc_symbols.insert("abs".to_string(), CSymbol {
            name: "abs".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // long labs(long x)
        libc_symbols.insert("labs".to_string(), CSymbol {
            name: "labs".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // long long llabs(long long x)
        libc_symbols.insert("llabs".to_string(), CSymbol {
            name: "llabs".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // div_t div(int numer, int denom)
        libc_symbols.insert("div".to_string(), CSymbol {
            name: "div".to_string(),
            parameters: vec![
                Parameter {
                    name: "numer".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "denom".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(), // div_t as int (struct)
            is_variadic: false,
        });

        // ldiv_t ldiv(long numer, long denom)
        libc_symbols.insert("ldiv".to_string(), CSymbol {
            name: "ldiv".to_string(),
            parameters: vec![
                Parameter {
                    name: "numer".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "denom".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // lldiv_t lldiv(long long numer, long long denom)
        libc_symbols.insert("lldiv".to_string(), CSymbol {
            name: "lldiv".to_string(),
            parameters: vec![
                Parameter {
                    name: "numer".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "denom".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int sprintf(char *str, const char *format, ...)
        libc_symbols.insert("sprintf".to_string(), CSymbol {
            name: "sprintf".to_string(),
            parameters: vec![
                Parameter {
                    name: "str".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "format".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int snprintf(char *str, int size, const char *format, ...)
        libc_symbols.insert("snprintf".to_string(), CSymbol {
            name: "snprintf".to_string(),
            parameters: vec![
                Parameter {
                    name: "str".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "format".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int sscanf(const char *str, const char *format, ...)
        libc_symbols.insert("sscanf".to_string(), CSymbol {
            name: "sscanf".to_string(),
            parameters: vec![
                Parameter {
                    name: "str".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "format".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int fscanf(FILE *stream, const char *format, ...)
        libc_symbols.insert("fscanf".to_string(), CSymbol {
            name: "fscanf".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "format".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int scanf(const char *format, ...)
        libc_symbols.insert("scanf".to_string(), CSymbol {
            name: "scanf".to_string(),
            parameters: vec![Parameter {
                name: "format".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // char *strdup(const char *s)
        libc_symbols.insert("strdup".to_string(), CSymbol {
            name: "strdup".to_string(),
            parameters: vec![Parameter {
                name: "s".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // char *strndup(const char *s, int n)
        libc_symbols.insert("strndup".to_string(), CSymbol {
            name: "strndup".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int strcasecmp(const char *s1, const char *s2)
        libc_symbols.insert("strcasecmp".to_string(), CSymbol {
            name: "strcasecmp".to_string(),
            parameters: vec![
                Parameter {
                    name: "s1".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "s2".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int strncasecmp(const char *s1, const char *s2, int n)
        libc_symbols.insert("strncasecmp".to_string(), CSymbol {
            name: "strncasecmp".to_string(),
            parameters: vec![
                Parameter {
                    name: "s1".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "s2".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int strcoll(const char *s1, const char *s2)
        libc_symbols.insert("strcoll".to_string(), CSymbol {
            name: "strcoll".to_string(),
            parameters: vec![
                Parameter {
                    name: "s1".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "s2".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int strspn(const char *s, const char *accept)
        libc_symbols.insert("strspn".to_string(), CSymbol {
            name: "strspn".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "accept".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int strcspn(const char *s, const char *reject)
        libc_symbols.insert("strcspn".to_string(), CSymbol {
            name: "strcspn".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "reject".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *strpbrk(const char *s, const char *accept)
        libc_symbols.insert("strpbrk".to_string(), CSymbol {
            name: "strpbrk".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "accept".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // char *strrchr(const char *s, int c)
        libc_symbols.insert("strrchr".to_string(), CSymbol {
            name: "strrchr".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "c".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int strxfrm(char *dest, const char *src, int n)
        libc_symbols.insert("strxfrm".to_string(), CSymbol {
            name: "strxfrm".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void *memchr(const void *s, int c, int n)
        libc_symbols.insert("memchr".to_string(), CSymbol {
            name: "memchr".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "c".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // void *memmove(void *dest, const void *src, int n)
        libc_symbols.insert("memmove".to_string(), CSymbol {
            name: "memmove".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // void *memccpy(void *dest, const void *src, int c, int n)
        libc_symbols.insert("memccpy".to_string(), CSymbol {
            name: "memccpy".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "c".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int getpid(void)
        libc_symbols.insert("getpid".to_string(), CSymbol {
            name: "getpid".to_string(),
            parameters: vec![],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int getppid(void)
        libc_symbols.insert("getppid".to_string(), CSymbol {
            name: "getppid".to_string(),
            parameters: vec![],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fork(void)
        libc_symbols.insert("fork".to_string(), CSymbol {
            name: "fork".to_string(),
            parameters: vec![],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int execv(const char *path, char *const argv[])
        libc_symbols.insert("execv".to_string(), CSymbol {
            name: "execv".to_string(),
            parameters: vec![
                Parameter {
                    name: "path".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "argv".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int execvp(const char *file, char *const argv[])
        libc_symbols.insert("execvp".to_string(), CSymbol {
            name: "execvp".to_string(),
            parameters: vec![
                Parameter {
                    name: "file".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "argv".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int execl(const char *path, const char *arg, ...)
        libc_symbols.insert("execl".to_string(), CSymbol {
            name: "execl".to_string(),
            parameters: vec![
                Parameter {
                    name: "path".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "arg".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int execlp(const char *file, const char *arg, ...)
        libc_symbols.insert("execlp".to_string(), CSymbol {
            name: "execlp".to_string(),
            parameters: vec![
                Parameter {
                    name: "file".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "arg".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int execle(const char *path, const char *arg, ...)
        libc_symbols.insert("execle".to_string(), CSymbol {
            name: "execle".to_string(),
            parameters: vec![
                Parameter {
                    name: "path".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "arg".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int execve(const char *pathname, char *const argv[], char *const envp[])
        libc_symbols.insert("execve".to_string(), CSymbol {
            name: "execve".to_string(),
            parameters: vec![
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "argv".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "envp".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void _exit(int status)
        libc_symbols.insert("_exit".to_string(), CSymbol {
            name: "_exit".to_string(),
            parameters: vec![Parameter {
                name: "status".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int wait(int *status)
        libc_symbols.insert("wait".to_string(), CSymbol {
            name: "wait".to_string(),
            parameters: vec![Parameter {
                name: "status".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int waitpid(int pid, int *status, int options)
        libc_symbols.insert("waitpid".to_string(), CSymbol {
            name: "waitpid".to_string(),
            parameters: vec![
                Parameter {
                    name: "pid".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "status".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "options".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int pipe(int pipefd[2])
        libc_symbols.insert("pipe".to_string(), CSymbol {
            name: "pipe".to_string(),
            parameters: vec![Parameter {
                name: "pipefd".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int dup(int oldfd)
        libc_symbols.insert("dup".to_string(), CSymbol {
            name: "dup".to_string(),
            parameters: vec![Parameter {
                name: "oldfd".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int dup2(int oldfd, int newfd)
        libc_symbols.insert("dup2".to_string(), CSymbol {
            name: "dup2".to_string(),
            parameters: vec![
                Parameter {
                    name: "oldfd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "newfd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int close(int fd)
        libc_symbols.insert("close".to_string(), CSymbol {
            name: "close".to_string(),
            parameters: vec![Parameter {
                name: "fd".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int read(int fd, void *buf, int count)
        libc_symbols.insert("read".to_string(), CSymbol {
            name: "read".to_string(),
            parameters: vec![
                Parameter {
                    name: "fd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "buf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "count".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int write(int fd, const void *buf, int count)
        libc_symbols.insert("write".to_string(), CSymbol {
            name: "write".to_string(),
            parameters: vec![
                Parameter {
                    name: "fd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "buf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "count".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int open(const char *pathname, int flags, ...)
        libc_symbols.insert("open".to_string(), CSymbol {
            name: "open".to_string(),
            parameters: vec![
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "flags".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int openat(int dirfd, const char *pathname, int flags, ...)
        libc_symbols.insert("openat".to_string(), CSymbol {
            name: "openat".to_string(),
            parameters: vec![
                Parameter {
                    name: "dirfd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "flags".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: true,
        });

        // int lseek(int fd, int offset, int whence)
        libc_symbols.insert("lseek".to_string(), CSymbol {
            name: "lseek".to_string(),
            parameters: vec![
                Parameter {
                    name: "fd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "whence".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int unlink(const char *pathname)
        libc_symbols.insert("unlink".to_string(), CSymbol {
            name: "unlink".to_string(),
            parameters: vec![Parameter {
                name: "pathname".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int rmdir(const char *pathname)
        libc_symbols.insert("rmdir".to_string(), CSymbol {
            name: "rmdir".to_string(),
            parameters: vec![Parameter {
                name: "pathname".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int mkdir(const char *pathname, int mode)
        libc_symbols.insert("mkdir".to_string(), CSymbol {
            name: "mkdir".to_string(),
            parameters: vec![
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "mode".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int chdir(const char *path)
        libc_symbols.insert("chdir".to_string(), CSymbol {
            name: "chdir".to_string(),
            parameters: vec![Parameter {
                name: "path".to_string(),
                param_type: "string".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *getcwd(char *buf, int size)
        libc_symbols.insert("getcwd".to_string(), CSymbol {
            name: "getcwd".to_string(),
            parameters: vec![
                Parameter {
                    name: "buf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int chmod(const char *pathname, int mode)
        libc_symbols.insert("chmod".to_string(), CSymbol {
            name: "chmod".to_string(),
            parameters: vec![
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "mode".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int stat(const char *pathname, int *statbuf)
        libc_symbols.insert("stat".to_string(), CSymbol {
            name: "stat".to_string(),
            parameters: vec![
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "statbuf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int lstat(const char *pathname, int *statbuf)
        libc_symbols.insert("lstat".to_string(), CSymbol {
            name: "lstat".to_string(),
            parameters: vec![
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "statbuf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fstat(int fd, int *statbuf)
        libc_symbols.insert("fstat".to_string(), CSymbol {
            name: "fstat".to_string(),
            parameters: vec![
                Parameter {
                    name: "fd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "statbuf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int access(const char *pathname, int mode)
        libc_symbols.insert("access".to_string(), CSymbol {
            name: "access".to_string(),
            parameters: vec![
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "mode".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int link(const char *oldpath, const char *newpath)
        libc_symbols.insert("link".to_string(), CSymbol {
            name: "link".to_string(),
            parameters: vec![
                Parameter {
                    name: "oldpath".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "newpath".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int symlink(const char *target, const char *linkpath)
        libc_symbols.insert("symlink".to_string(), CSymbol {
            name: "symlink".to_string(),
            parameters: vec![
                Parameter {
                    name: "target".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "linkpath".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int readlink(const char *pathname, char *buf, int bufsiz)
        libc_symbols.insert("readlink".to_string(), CSymbol {
            name: "readlink".to_string(),
            parameters: vec![
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "buf".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "bufsiz".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int unlinkat(int dirfd, const char *pathname, int flags)
        libc_symbols.insert("unlinkat".to_string(), CSymbol {
            name: "unlinkat".to_string(),
            parameters: vec![
                Parameter {
                    name: "dirfd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "pathname".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "flags".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int usleep(int usec)
        libc_symbols.insert("usleep".to_string(), CSymbol {
            name: "usleep".to_string(),
            parameters: vec![Parameter {
                name: "usec".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // unsigned int sleep(unsigned int seconds)
        libc_symbols.insert("sleep".to_string(), CSymbol {
            name: "sleep".to_string(),
            parameters: vec![Parameter {
                name: "seconds".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void *dlopen(const char *filename, int flags)
        libc_symbols.insert("dlopen".to_string(), CSymbol {
            name: "dlopen".to_string(),
            parameters: vec![
                Parameter {
                    name: "filename".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "flags".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int dlclose(void *handle)
        libc_symbols.insert("dlclose".to_string(), CSymbol {
            name: "dlclose".to_string(),
            parameters: vec![Parameter {
                name: "handle".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void *dlsym(void *handle, const char *symbol)
        libc_symbols.insert("dlsym".to_string(), CSymbol {
            name: "dlsym".to_string(),
            parameters: vec![
                Parameter {
                    name: "handle".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "symbol".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // char *dlerror(void)
        libc_symbols.insert("dlerror".to_string(), CSymbol {
            name: "dlerror".to_string(),
            parameters: vec![],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int atexit(void (*function)(void))
        libc_symbols.insert("atexit".to_string(), CSymbol {
            name: "atexit".to_string(),
            parameters: vec![Parameter {
                name: "function".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void exit(int status)
        libc_symbols.insert("exit".to_string(), CSymbol {
            name: "exit".to_string(),
            parameters: vec![Parameter {
                name: "status".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int isatty(int fd)
        libc_symbols.insert("isatty".to_string(), CSymbol {
            name: "isatty".to_string(),
            parameters: vec![Parameter {
                name: "fd".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fileno(FILE *stream)
        libc_symbols.insert("fileno".to_string(), CSymbol {
            name: "fileno".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // FILE *fdopen(int fd, const char *mode)
        libc_symbols.insert("fdopen".to_string(), CSymbol {
            name: "fdopen".to_string(),
            parameters: vec![
                Parameter {
                    name: "fd".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "mode".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int fgetc(FILE *stream)
        libc_symbols.insert("fgetc".to_string(), CSymbol {
            name: "fgetc".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fputc(int c, FILE *stream)
        libc_symbols.insert("fputc".to_string(), CSymbol {
            name: "fputc".to_string(),
            parameters: vec![
                Parameter {
                    name: "c".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *fgets(char *s, int size, FILE *stream)
        libc_symbols.insert("fgets".to_string(), CSymbol {
            name: "fgets".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(),
            is_variadic: false,
        });

        // int fputs(const char *s, FILE *stream)
        libc_symbols.insert("fputs".to_string(), CSymbol {
            name: "fputs".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int ungetc(int c, FILE *stream)
        libc_symbols.insert("ungetc".to_string(), CSymbol {
            name: "ungetc".to_string(),
            parameters: vec![
                Parameter {
                    name: "c".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream)
        libc_symbols.insert("fread".to_string(), CSymbol {
            name: "fread".to_string(),
            parameters: vec![
                Parameter {
                    name: "ptr".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "nmemb".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream)
        libc_symbols.insert("fwrite".to_string(), CSymbol {
            name: "fwrite".to_string(),
            parameters: vec![
                Parameter {
                    name: "ptr".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "nmemb".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fseek(FILE *stream, long offset, int whence)
        libc_symbols.insert("fseek".to_string(), CSymbol {
            name: "fseek".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "whence".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // long ftell(FILE *stream)
        libc_symbols.insert("ftell".to_string(), CSymbol {
            name: "ftell".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void rewind(FILE *stream)
        libc_symbols.insert("rewind".to_string(), CSymbol {
            name: "rewind".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int fgetpos(FILE *stream, int *pos)
        libc_symbols.insert("fgetpos".to_string(), CSymbol {
            name: "fgetpos".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "pos".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fsetpos(FILE *stream, const int *pos)
        libc_symbols.insert("fsetpos".to_string(), CSymbol {
            name: "fsetpos".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "pos".to_string(),
                    param_type: "object".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void clearerr(FILE *stream)
        libc_symbols.insert("clearerr".to_string(), CSymbol {
            name: "clearerr".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "void".to_string(),
            is_variadic: false,
        });

        // int feof(FILE *stream)
        libc_symbols.insert("feof".to_string(), CSymbol {
            name: "feof".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int ferror(FILE *stream)
        libc_symbols.insert("ferror".to_string(), CSymbol {
            name: "ferror".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fflush(FILE *stream)
        libc_symbols.insert("fflush".to_string(), CSymbol {
            name: "fflush".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "object".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *strcpy(char *dest, const char *src)
        libc_symbols.insert("strcpy".to_string(), CSymbol {
            name: "strcpy".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "object".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "object".to_string(), // pointer as int
            is_variadic: false,
        });

        // int abs(int j)
        libc_symbols.insert("abs".to_string(), CSymbol {
            name: "abs".to_string(),
            parameters: vec![Parameter {
                name: "j".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        builtin_symbols.insert("libc".to_string(), CSymbolTable {
            library: "libc".to_string(),
            symbols: libc_symbols,
        });

        // libm数学函数
        let mut libm_symbols = HashMap::new();

        // double sin(double x)
        libm_symbols.insert("sin".to_string(), CSymbol {
            name: "sin".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double cos(double x)
        libm_symbols.insert("cos".to_string(), CSymbol {
            name: "cos".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double sqrt(double x)
        libm_symbols.insert("sqrt".to_string(), CSymbol {
            name: "sqrt".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double pow(double x, double y)
        libm_symbols.insert("pow".to_string(), CSymbol {
            name: "pow".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double fabs(double x)
        libm_symbols.insert("fabs".to_string(), CSymbol {
            name: "fabs".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double tan(double x)
        libm_symbols.insert("tan".to_string(), CSymbol {
            name: "tan".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double asin(double x)
        libm_symbols.insert("asin".to_string(), CSymbol {
            name: "asin".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double acos(double x)
        libm_symbols.insert("acos".to_string(), CSymbol {
            name: "acos".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double atan(double x)
        libm_symbols.insert("atan".to_string(), CSymbol {
            name: "atan".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double atan2(double y, double x)
        libm_symbols.insert("atan2".to_string(), CSymbol {
            name: "atan2".to_string(),
            parameters: vec![
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double sinh(double x)
        libm_symbols.insert("sinh".to_string(), CSymbol {
            name: "sinh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double cosh(double x)
        libm_symbols.insert("cosh".to_string(), CSymbol {
            name: "cosh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double tanh(double x)
        libm_symbols.insert("tanh".to_string(), CSymbol {
            name: "tanh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double asinh(double x)
        libm_symbols.insert("asinh".to_string(), CSymbol {
            name: "asinh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double acosh(double x)
        libm_symbols.insert("acosh".to_string(), CSymbol {
            name: "acosh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double atanh(double x)
        libm_symbols.insert("atanh".to_string(), CSymbol {
            name: "atanh".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double exp(double x)
        libm_symbols.insert("exp".to_string(), CSymbol {
            name: "exp".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double exp2(double x)
        libm_symbols.insert("exp2".to_string(), CSymbol {
            name: "exp2".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double expm1(double x)
        libm_symbols.insert("expm1".to_string(), CSymbol {
            name: "expm1".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double log(double x)
        libm_symbols.insert("log".to_string(), CSymbol {
            name: "log".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double log10(double x)
        libm_symbols.insert("log10".to_string(), CSymbol {
            name: "log10".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double log2(double x)
        libm_symbols.insert("log2".to_string(), CSymbol {
            name: "log2".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double log1p(double x)
        libm_symbols.insert("log1p".to_string(), CSymbol {
            name: "log1p".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double ceil(double x)
        libm_symbols.insert("ceil".to_string(), CSymbol {
            name: "ceil".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double floor(double x)
        libm_symbols.insert("floor".to_string(), CSymbol {
            name: "floor".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double round(double x)
        libm_symbols.insert("round".to_string(), CSymbol {
            name: "round".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double trunc(double x)
        libm_symbols.insert("trunc".to_string(), CSymbol {
            name: "trunc".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double fmod(double x, double y)
        libm_symbols.insert("fmod".to_string(), CSymbol {
            name: "fmod".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double remainder(double x, double y)
        libm_symbols.insert("remainder".to_string(), CSymbol {
            name: "remainder".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double fmax(double x, double y)
        libm_symbols.insert("fmax".to_string(), CSymbol {
            name: "fmax".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double fmin(double x, double y)
        libm_symbols.insert("fmin".to_string(), CSymbol {
            name: "fmin".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double fdim(double x, double y)
        libm_symbols.insert("fdim".to_string(), CSymbol {
            name: "fdim".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double hypot(double x, double y)
        libm_symbols.insert("hypot".to_string(), CSymbol {
            name: "hypot".to_string(),
            parameters: vec![
                Parameter {
                    name: "x".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "y".to_string(),
                    param_type: "float(8)".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double sqrt(double x)
        libm_symbols.insert("sqrt".to_string(), CSymbol {
            name: "sqrt".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        // double cbrt(double x)
        libm_symbols.insert("cbrt".to_string(), CSymbol {
            name: "cbrt".to_string(),
            parameters: vec![Parameter {
                name: "x".to_string(),
                param_type: "float(8)".to_string(),
                is_variadic: false,
            }],
            return_type: "float(8)".to_string(),
            is_variadic: false,
        });

        builtin_symbols.insert("libm".to_string(), CSymbolTable {
            library: "libm".to_string(),
            symbols: libm_symbols,
        });


    builtin_symbols
}
