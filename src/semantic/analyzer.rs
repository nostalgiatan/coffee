use super::scope::ScopeSpace;
use super::lifetime::LifetimeSpace;
use super::symbols::SymbolSpace;
use crate::types::definition::*;
use crate::types::registry::TypeRegistry;
use crate::types::errors::TypeSystemError;
use crate::c::{CSymbolTable, CSymbol};
use crate::parser::function::Parameter;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

//=============================================================================
// Constant Folding Types
//=============================================================================

/// Result of constant folding
#[derive(Debug, Clone, PartialEq)]
pub enum ConstantValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
}

impl ConstantValue {
    /// Get type name of the constant
    pub fn type_name(&self) -> &'static str {
        match self {
            ConstantValue::Int(_) => "int",
            ConstantValue::Float(_) => "float",
            ConstantValue::Bool(_) => "bool",
            ConstantValue::String(_) => "string",
        }
    }

    /// Convert to LLVM-style string representation
    pub fn to_llvm_literal(&self) -> String {
        match self {
            ConstantValue::Int(i) => i.to_string(),
            ConstantValue::Float(f) => f.to_string(),
            ConstantValue::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
            ConstantValue::String(s) => format!("\"{}\"", s),
        }
    }
}

//=============================================================================
// Semantic Analyzer
//=============================================================================

/// 语义分析器：协调所有语义分析空间
pub struct SemanticAnalyzer {
    /// 作用域管理
    scope_space: Arc<RwLock<ScopeSpace>>,
    /// 生命周期管理
    lifetime_space: Arc<RwLock<LifetimeSpace>>,
    /// 符号管理
    symbol_space: Arc<RwLock<SymbolSpace>>,
    /// 类型注册器
    type_registry: Arc<RwLock<TypeRegistry>>,
    /// 分析错误
    errors: Vec<TypeSystemError>,
    /// C库导入（格式："library:symbol"，这些符号不需要在语义分析阶段报错）
    c_imports: Arc<RwLock<Vec<String>>>,
    /// C函数符号表（用于类型检查C函数调用）
    cfc_symbols: Arc<RwLock<HashMap<String, CSymbolTable>>>,
    /// 当前函数的返回类型（用于检查 void 函数的 return 语句）
    current_function_return_type: Option<String>,
    /// 当前函数的名字（用于更好的错误提示）
    current_function_name: Option<String>,
}

/// Calculate Levenshtein distance between two strings
/// Used for suggesting similar symbol names when a symbol is not found
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    // Initialize first row and column
    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    // Fill the matrix
    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            matrix[i][j] = std::cmp::min(
                std::cmp::min(
                    matrix[i - 1][j] + 1,      // deletion
                    matrix[i][j - 1] + 1),      // insertion
                matrix[i - 1][j - 1] + cost);  // substitution
        }
    }

    matrix[a_len][b_len]
}

impl SemanticAnalyzer {
    /// 创建新的语义分析器
    pub fn new(type_registry: Arc<RwLock<TypeRegistry>>) -> Self {
        let mut analyzer = SemanticAnalyzer {
            scope_space: Arc::new(RwLock::new(ScopeSpace::root())),
            lifetime_space: Arc::new(RwLock::new(LifetimeSpace::new("global"))),
            symbol_space: Arc::new(RwLock::new(SymbolSpace::new("global"))),
            type_registry,
            errors: Vec::new(),
            c_imports: Arc::new(RwLock::new(Vec::new())),
            cfc_symbols: Arc::new(RwLock::new(HashMap::new())),
            current_function_return_type: None,
            current_function_name: None,
        };

        // 初始化内置的C函数签名（libc和libm）
        analyzer.init_builtin_c_symbols();

        analyzer
    }

    /// 初始化内置的C函数签名（libc和libm）
    fn init_builtin_c_symbols(&mut self) {
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
            return_type: "int(4)+".to_string(), // pointer as int
            is_variadic: false,
        });

        // void free(void *ptr)
        libc_symbols.insert("free".to_string(), CSymbol {
            name: "free".to_string(),
            parameters: vec![Parameter {
                name: "ptr".to_string(),
                param_type: "int".to_string(), // pointer as int
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

        // int fprintf(int stream, const char *format, ...)
        libc_symbols.insert("fprintf".to_string(), CSymbol {
            name: "fprintf".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(), // pointer as int
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
            return_type: "int(4)+".to_string(), // pointer as int
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
            return_type: "int(4)+".to_string(), // pointer as int
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
            return_type: "int(4)+".to_string(), // pointer as int
            is_variadic: false,
        });

        // void *memcpy(void *dest, const void *src, int n)
        libc_symbols.insert("memcpy".to_string(), CSymbol {
            name: "memcpy".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "int".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "int".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(), // pointer as int
            is_variadic: false,
        });

        // void *memset(void *s, int c, int n)
        libc_symbols.insert("memset".to_string(), CSymbol {
            name: "memset".to_string(),
            parameters: vec![
                Parameter {
                    name: "s".to_string(),
                    param_type: "int".to_string(), // pointer as int
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
            return_type: "int(4)+".to_string(), // pointer as int
            is_variadic: false,
        });

        // int memcmp(const void *ptr1, const void *ptr2, int n)
        libc_symbols.insert("memcmp".to_string(), CSymbol {
            name: "memcmp".to_string(),
            parameters: vec![
                Parameter {
                    name: "ptr1".to_string(),
                    param_type: "int".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "ptr2".to_string(),
                    param_type: "int".to_string(), // pointer as int
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
            return_type: "int(4)+".to_string(), // pointer as int
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
                param_type: "int".to_string(), // pointer as int
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *strncat(char *dest, const char *src, int n)
        libc_symbols.insert("strncat".to_string(), CSymbol {
            name: "strncat".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "int".to_string(),
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

        // char *strcpy(char *dest, const char *src)
        libc_symbols.insert("strcpy".to_string(), CSymbol {
            name: "strcpy".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
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
            return_type: "int(4)+".to_string(), // FILE* as int
            is_variadic: false,
        });

        // int fclose(FILE *stream)
        libc_symbols.insert("fclose".to_string(), CSymbol {
            name: "fclose".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "int".to_string(), // FILE* as int
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

        // long ftell(FILE *stream)
        libc_symbols.insert("ftell".to_string(), CSymbol {
            name: "ftell".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *tmpnam(char *s)
        libc_symbols.insert("tmpnam".to_string(), CSymbol {
            name: "tmpnam".to_string(),
            parameters: vec![Parameter {
                name: "s".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int setvbuf(FILE *stream, char *buf, int mode, int size)
        libc_symbols.insert("setvbuf".to_string(), CSymbol {
            name: "setvbuf".to_string(),
            parameters: vec![
                Parameter {
                    name: "stream".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "buf".to_string(),
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "buf".to_string(),
                    param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
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
                    param_type: "int".to_string(),
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
            return_type: "int(4)+".to_string(), // pointer as int
            is_variadic: false,
        });

        // void free(void *ptr)
        libc_symbols.insert("free".to_string(), CSymbol {
            name: "free".to_string(),
            parameters: vec![Parameter {
                name: "ptr".to_string(),
                param_type: "int".to_string(),
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
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void *realloc(void *ptr, int size)
        libc_symbols.insert("realloc".to_string(), CSymbol {
            name: "realloc".to_string(),
            parameters: vec![
                Parameter {
                    name: "ptr".to_string(),
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

        // int qsort comparison function type - not directly callable
        // void qsort(void *base, int nmemb, int size, int (*compar)(const void *, const void *))
        libc_symbols.insert("qsort".to_string(), CSymbol {
            name: "qsort".to_string(),
            parameters: vec![
                Parameter {
                    name: "base".to_string(),
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(), // function pointer as int
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "base".to_string(),
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
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

        // int snprintf(char *str, int size, const char *format, ...)
        libc_symbols.insert("snprintf".to_string(), CSymbol {
            name: "snprintf".to_string(),
            parameters: vec![
                Parameter {
                    name: "str".to_string(),
                    param_type: "int".to_string(),
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
            return_type: "int(4)+".to_string(),
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
            return_type: "int(4)+".to_string(),
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
            return_type: "int(4)+".to_string(),
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
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int strxfrm(char *dest, const char *src, int n)
        libc_symbols.insert("strxfrm".to_string(), CSymbol {
            name: "strxfrm".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // void *memmove(void *dest, const void *src, int n)
        libc_symbols.insert("memmove".to_string(), CSymbol {
            name: "memmove".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "int".to_string(),
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

        // void *memccpy(void *dest, const void *src, int c, int n)
        libc_symbols.insert("memccpy".to_string(), CSymbol {
            name: "memccpy".to_string(),
            parameters: vec![
                Parameter {
                    name: "dest".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "int".to_string(),
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
            return_type: "int(4)+".to_string(),
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
                param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int dlclose(void *handle)
        libc_symbols.insert("dlclose".to_string(), CSymbol {
            name: "dlclose".to_string(),
            parameters: vec![Parameter {
                name: "handle".to_string(),
                param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "symbol".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // char *dlerror(void)
        libc_symbols.insert("dlerror".to_string(), CSymbol {
            name: "dlerror".to_string(),
            parameters: vec![],
            return_type: "int(4)+".to_string(),
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
                param_type: "int".to_string(),
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
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        });

        // int fgetc(FILE *stream)
        libc_symbols.insert("fgetc".to_string(), CSymbol {
            name: "fgetc".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "size".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "stream".to_string(),
                    param_type: "int".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
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

        // long ftell(FILE *stream)
        libc_symbols.insert("ftell".to_string(), CSymbol {
            name: "ftell".to_string(),
            parameters: vec![Parameter {
                name: "stream".to_string(),
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "pos".to_string(),
                    param_type: "int".to_string(),
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
                    param_type: "int".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "pos".to_string(),
                    param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                param_type: "int".to_string(),
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
                    param_type: "int".to_string(), // pointer as int
                    is_variadic: false,
                },
                Parameter {
                    name: "src".to_string(),
                    param_type: "string".to_string(),
                    is_variadic: false,
                }
            ],
            return_type: "int(4)+".to_string(), // pointer as int
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

        // 设置内置符号表
        if let Ok(mut cfc_symbols) = self.cfc_symbols.write() {
            *cfc_symbols = builtin_symbols;
        }
    }

    /// 获取作用域空间
    pub fn scope_space(&self) -> Arc<RwLock<ScopeSpace>> {
        Arc::clone(&self.scope_space)
    }

    /// 获取生命周期空间
    pub fn lifetime_space(&self) -> Arc<RwLock<LifetimeSpace>> {
        Arc::clone(&self.lifetime_space)
    }

    /// 获取符号空间
    pub fn symbol_space(&self) -> Arc<RwLock<SymbolSpace>> {
        Arc::clone(&self.symbol_space)
    }

    /// 获取类型注册器
    pub fn type_registry(&self) -> Arc<RwLock<TypeRegistry>> {
        Arc::clone(&self.type_registry)
    }

    /// 分析变量声明
    pub fn analyze_variable_decl(&mut self, decl: &crate::parser::var::VariableDecl) -> Result<(), TypeSystemError> {
        let span = Span::new(0, decl.name.len());

        // 解析类型
        let ty = match self.type_registry.read() {
            Ok(reg) => reg.resolve_type(&decl.var_type)?,
            Err(_) => return Err(TypeSystemError::ParseError {
                type_str: decl.var_type.clone(),
                reason: "Failed to access type registry".to_string(),
            }),
        };

        // 创建实体
        let entity = Entity::Variable {
            ty,
            initialized: !decl.value.is_empty(),
        };

        // 在作用域中绑定
        // 不在作用域空间中绑定变量，只在符号空间中声明
        // 这样可以避免不同函数中的变量冲突
        // if let Ok(scope) = self.scope_space.write() {
        //     scope.bind(decl.name.clone(), entity.clone(), span)?;
        // }

        // 在符号空间中声明
        // 如果在函数内，使用限定名（function::variable）以避免冲突
        {
            let symbol_name = if let Some(ref func_name) = self.current_function_name {
                format!("{}::{}", func_name, decl.name)
            } else {
                decl.name.clone()
            };

            if let Ok(mut symbols) = self.symbol_space.write() {
                let binding = Binding {
                    name: symbol_name.clone(),
                    entity,
                    span,
                    mutable: false,
                    visibility: Visibility::Private,
                };
                symbols.declare(symbol_name, binding)?;
            }
        }

        // 分析初始化值的表达式（用于检查未定义符号）
        if !decl.value.is_empty() {
            // 解析值字符串为表达式并分析
            if let Ok(expr) = crate::parser::expr::parse_expression(&decl.value) {
                // 分析表达式以检查未定义符号
                // 注意：我们需要先释放写锁，然后调用 analyze_expression
                if let Err(e) = self.analyze_expression(&expr) {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// 声明函数（不分析函数体）
    /// 这个函数只声明函数签名，不分析函数体
    /// 用于支持互递归函数
    pub fn declare_function_decl(&mut self, func: &crate::parser::function::Function) -> Result<(), TypeSystemError> {
        let span = Span::new(0, func.name.len());

        // 检查函数是否已经被声明
        let function_already_declared = if let Ok(symbols) = self.symbol_space.read() {
            symbols.lookup(&func.name).is_some()
        } else {
            false
        };

        if function_already_declared {
            eprintln!("DEBUG: declare_function_decl: function '{}' already declared, skipping", func.name);
            return Ok(());
        }

        // 获取类型注册器读锁一次（避免在循环中重复获取）
        let type_registry = self.type_registry.read()
            .map_err(|_| TypeSystemError::ParseError {
                type_str: func.name.clone(),
                reason: "Failed to access type registry".to_string(),
            })?;

        // 解析参数类型
        let mut param_types = Vec::new();
        for (_idx, param) in func.parameters.iter().enumerate() {
            let ty = type_registry.resolve_type(&param.param_type)?;
            param_types.push(ty);
        }

        // 解析返回类型
        let return_type = type_registry.resolve_type(&func.return_type)?;

        // 释放锁后再进行后续操作
        drop(type_registry);

        // 创建函数实体
        let entity = Entity::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
            generics: vec![],
        };

        // 在作用域中绑定
        if let Ok(scope) = self.scope_space.write() {
            scope.bind(func.name.clone(), entity.clone(), span)?;
        }

        // 在符号空间中声明
        if let Ok(mut symbols) = self.symbol_space.write() {
            let binding = Binding {
                name: func.name.clone(),
                entity,
                span,
                mutable: false,
                visibility: Visibility::Public,
            };
            symbols.declare(func.name.clone(), binding)?;
        }

        // 在类型注册器中注册函数类型
        let func_type = Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        };

        if let Ok(registry) = self.type_registry.write() {
            registry.define_alias(func.name.clone(), func_type)?;
        }

        eprintln!("DEBUG: declare_function_decl: declared function '{}'", func.name);
        Ok(())
    }

    /// 分析函数声明
    pub fn analyze_function_decl(&mut self, func: &crate::parser::function::Function) -> Result<(), TypeSystemError> {
        let span = Span::new(0, func.name.len());

        // 获取类型注册器读锁一次（避免在循环中重复获取）
        let type_registry = self.type_registry.read()
            .map_err(|_| TypeSystemError::ParseError {
                type_str: func.name.clone(),
                reason: "Failed to access type registry".to_string(),
            })?;

        // 解析参数类型
        let mut param_types = Vec::new();
        for (_idx, param) in func.parameters.iter().enumerate() {
            let ty = type_registry.resolve_type(&param.param_type)?;
            param_types.push(ty);
        }

        // 解析返回类型
        let return_type = type_registry.resolve_type(&func.return_type)?;

        // 释放锁后再进行后续操作
        drop(type_registry);

        // 创建函数实体
        let entity = Entity::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
            generics: vec![],
        };

        // 在类型注册器中注册函数类型
        let func_type = Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        };

        // 检查函数是否已经被声明（在 declare_function_decl 中）
        // 如果已经声明，跳过声明步骤，直接进入函数体分析
        let function_already_declared = if let Ok(symbols) = self.symbol_space.read() {
            symbols.lookup(&func.name).is_some()
        } else {
            false
        };

        eprintln!("DEBUG: analyze_function_decl: function_already_declared={} for function '{}'", function_already_declared, func.name);

        if !function_already_declared {
            // 在作用域中绑定
            if let Ok(scope) = self.scope_space.write() {
                scope.bind(func.name.clone(), entity.clone(), span)?;
            }

            // 在符号空间中声明
            if let Ok(mut symbols) = self.symbol_space.write() {
                let binding = Binding {
                    name: func.name.clone(),
                    entity,
                    span,
                    mutable: false,
                    visibility: Visibility::Public,
                };
                symbols.declare(func.name.clone(), binding)?;
            }

            if let Ok(registry) = self.type_registry.write() {
                registry.define_alias(func.name.clone(), func_type.clone())?;
            }
        }

        // 设置当前函数名称，以便在分析函数体内的表达式时使用限定名查找
        self.current_function_name = Some(func.name.clone());

        // 在函数作用域中绑定参数
        // 注意：这里需要重新获取 type_registry，因为之前已经 drop 了
        let type_registry = self.type_registry.read()
            .map_err(|_| TypeSystemError::ParseError {
                type_str: func.name.clone(),
                reason: "Failed to access type registry".to_string(),
            })?;

        eprintln!("DEBUG: analyze_function_decl: binding parameters for function '{}', params: {:?}", func.name, func.parameters.iter().map(|p| &p.name).collect::<Vec<_>>());

        // 在符号空间中绑定参数，使用限定名（function::parameter）
        if let Ok(mut symbols) = self.symbol_space.write() {
            for param in &func.parameters {
                let param_type = type_registry.resolve_type(&param.param_type)?;
                let param_entity = Entity::Variable {
                    ty: param_type.clone(),
                    initialized: true,
                };
                let qualified_name = format!("{}::{}", func.name, param.name);
                eprintln!("DEBUG: analyze_function_decl: binding parameter '{}' as '{}' in function '{}'", param.name, qualified_name, func.name);
                let binding = Binding {
                    name: qualified_name.clone(),
                    entity: param_entity,
                    span,
                    mutable: false,
                    visibility: Visibility::Private,
                };
                symbols.declare(qualified_name, binding)?;
            }
        }

        drop(type_registry);

        // Note: func_type is already registered in declare_function_decl (first pass)
        // We don't need to register it again here

        // Create a child scope for the function
        let function_scope = self.enter_scope(&func.name);

        // Add function parameters to the function scope
        for param in &func.parameters {
            let param_span = Span::new(0, param.name.len());
            let param_type = match self.type_registry.read() {
                Ok(reg) => reg.resolve_type(&param.param_type)?,
                Err(_) => return Err(TypeSystemError::ParseError {
                    type_str: param.param_type.clone(),
                    reason: "Failed to access type registry".to_string(),
                }),
            };

            let param_entity = Entity::Variable {
                ty: param_type,
                initialized: true,
            };

            // Add to function scope (not global scope)
            if let Ok(scope) = function_scope.write() {
                scope.bind(param.name.clone(), param_entity.clone(), param_span)?;
            }

            // Note: Parameters are already bound to symbol_space with qualified names (function::parameter)
            // earlier in this function (lines 3653-3670), so we don't need to bind them again here
        }

        // Analyze function body statements to check for undefined symbols
        // Set current function return type and name for void return validation
        let old_return_type = self.current_function_return_type.clone();
        let old_function_name = self.current_function_name.clone();
        self.current_function_return_type = Some(func.return_type.clone());
        self.current_function_name = Some(func.name.clone());

        if let crate::parser::function::FunctionBody::Block(statements) = &func.body {
            for stmt in statements {
                self.analyze_statement(stmt)?;
            }
        }

        // Restore previous return type and function name
        self.current_function_return_type = old_return_type;
        self.current_function_name = old_function_name;

        // Clean up function-local variables from symbol space
        // Remove variables with qualified names (function::variable)
        if let Ok(mut symbols) = self.symbol_space.write() {
            let qualified_prefix = format!("{}::", func.name);
            let keys_to_remove: Vec<String> = symbols.declared_symbols()
                .iter()
                .filter(|name| name.starts_with(&qualified_prefix))
                .cloned()
                .collect();
            
            for key in keys_to_remove {
                let _ = symbols.remove(&key);
            }
        }

        Ok(())
    }

    /// 分析类型定义
    pub fn analyze_type_def(&mut self, name: &str, type_def: TypeDef) -> Result<(), TypeSystemError> {
        let span = Span::new(0, name.len());

        // 在类型注册器中注册
        if let Ok(registry) = self.type_registry.write() {
            registry.define_type(name, type_def.clone())?;
        }

        // 创建类型实体
        let entity = Entity::TypeDef { def: type_def };

        // 在符号空间中声明
        if let Ok(mut symbols) = self.symbol_space.write() {
            let binding = Binding {
                name: name.to_string(),
                entity,
                span,
                mutable: false,
                visibility: Visibility::Public,
            };
            symbols.declare(name.to_string(), binding)?;
        }

        Ok(())
    }

    /// 分析符号引用
    pub fn analyze_symbol_reference(&mut self, name: &str, span: Span) -> Result<Entity, TypeSystemError> {
        // 在符号空间中记录引用
        if let Ok(mut symbols) = self.symbol_space.write() {
            symbols.reference(name.to_string(), span);

            // 查找符号声明
            if let Some(binding) = symbols.lookup(name) {
                return Ok(binding.entity.clone());
            }
        }

        // 如果符号空间中没有，尝试作用域空间
        if let Ok(scope) = self.scope_space.read() {
            if let Some(binding) = scope.get(name) {
                return Ok(binding.entity);
            }
        }

        Err(TypeSystemError::NotFound {
            name: name.to_string(),
            kind: SpaceKind::Symbol,
        })
    }

    /// 进入新的作用域
    pub fn enter_scope(&self, name: &str) -> Arc<RwLock<ScopeSpace>> {
        if let Ok(scope) = self.scope_space.read() {
            let child = scope.child(name);
            Arc::new(RwLock::new((*child).clone()))
        } else {
            Arc::new(RwLock::new(ScopeSpace::new(name)))
        }
    }

    /// 获取分析错误
    pub fn errors(&self) -> &[TypeSystemError] {
        &self.errors
    }

    /// 添加错误
    pub fn add_error(&mut self, error: TypeSystemError) {
        self.errors.push(error);
    }

    /// 清空错误
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// 设置C库导入列表
    pub fn set_c_imports(&self, imports: Vec<String>) {
        if let Ok(mut c_imports) = self.c_imports.write() {
            *c_imports = imports;
        }
    }

    /// 设置C函数符号表（会与内置符号合并）
    pub fn set_cfc_symbols(&self, symbols: HashMap<String, CSymbolTable>) {
        if let Ok(mut cfc_symbols) = self.cfc_symbols.write() {
            // 合并符号表，而不是覆盖内置符号
            for (library, symbol_table) in symbols.iter() {
                cfc_symbols.insert(library.clone(), symbol_table.clone());
            }
        }
    }

    /// 获取C函数符号表
    pub fn get_cfc_symbols(&self) -> Result<HashMap<String, CSymbolTable>, TypeSystemError> {
        self.cfc_symbols.read()
            .map(|symbols| symbols.clone())
            .map_err(|_| TypeSystemError::ParseError {
                type_str: "cfc_symbols".to_string(),
                reason: "Failed to access cfc_symbols".to_string(),
            })
    }

    /// 获取C函数符号
    fn get_c_symbol(&self, symbol_name: &str) -> Option<crate::c::CSymbol> {
        if let Ok(cfc_symbols) = self.cfc_symbols.read() {
            for (_library, symbol_table) in cfc_symbols.iter() {
                if let Some(symbol) = symbol_table.symbols.get(symbol_name) {
                    return Some(symbol.clone());
                }
            }
        }
        None
    }

    /// 检查符号是否是C库导入
    fn is_c_import(&self, symbol_name: &str) -> bool {
        // 首先检查c_imports列表
        if let Ok(c_imports) = self.c_imports.read() {
            for c_import in c_imports.iter() {
                if c_import.contains(':') {
                    // Format is "library:symbol"
                    let parts: Vec<&str> = c_import.split(':').collect();
                    if parts.len() == 2 && parts[1] == symbol_name {
                        return true;
                    }
                } else if c_import == symbol_name {
                    // Direct match
                    return true;
                }
            }
        }

        // 然后检查是否是内置的C函数（libc/libm）
        if self.get_c_symbol(symbol_name).is_some() {
            return true;
        }

        false
    }

    /// 检查符号是否是命令行参数 (arg1, arg2, etc.)
    fn is_command_line_arg(&self, symbol_name: &str) -> bool {
        let symbol_name = symbol_name.trim();

        // Check if it matches arg1, arg2, ..., argN pattern
        if symbol_name.starts_with("arg") {
            let num_str = &symbol_name[3..]; // Skip "arg"
            if let Ok(num) = num_str.parse::<u32>() {
                // Valid if it's arg1 through arg127
                return num >= 1 && num <= 127;
            }
        }

        false
    }

    /// 执行完整的语义分析
    pub fn analyze(&mut self, program: &crate::parser::Program) -> Result<(), Vec<TypeSystemError>> {
        self.clear_errors();

        // 分析所有顶级声明
        for stmt in &program.statements {
            if let Err(error) = self.analyze_statement(stmt) {
                self.add_error(error);
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    /// 分析语句
    pub fn analyze_statement(&mut self, stmt: &crate::parser::Statement) -> Result<(), TypeSystemError> {
        match stmt {
            crate::parser::Statement::VariableDecl(decl) => {
                self.analyze_variable_decl(decl)
            }
            crate::parser::Statement::Function(func) => {
                // Check for import statements inside function body
                if let crate::parser::function::FunctionBody::Block(statements) = &func.body {
                    for stmt in statements {
                        if matches!(stmt, crate::parser::Statement::Import(_)) {
                            return Err(TypeSystemError::ParseError {
                                type_str: "import statement".to_string(),
                                reason: format!("import statements are not allowed inside function '{}'\n  = note: move the import statement to the top of the file, before any function definitions", func.name),
                            });
                        }
                    }
                }
                self.analyze_function_decl(func)
            }
            crate::parser::Statement::Class(class) => {
                // 先获取 type_def，这会在 block 结束时释放读锁
                let type_def = match self.type_registry.read() {
                    Ok(reg) => reg.bind_class(class)?,
                    Err(_) => return Err(TypeSystemError::ParseError {
                        type_str: class.name.clone(),
                        reason: "Failed to access type registry".to_string(),
                    }),
                };
                // 读锁已释放，现在可以安全调用 analyze_type_def（它会尝试获取写锁）
                self.analyze_type_def(&class.name, type_def)
            }
            crate::parser::Statement::Enum(enum_def) => {
                let type_def = match self.type_registry.read() {
                    Ok(reg) => reg.bind_enum(enum_def)?,
                    Err(_) => return Err(TypeSystemError::ParseError {
                        type_str: enum_def.name.clone(),
                        reason: "Failed to access type registry".to_string(),
                    }),
                };
                self.analyze_type_def(&enum_def.name, type_def)
            }
            crate::parser::Statement::Expr(expr) => {
                self.analyze_expression(expr)
            }
            crate::parser::Statement::Return(ret_stmt) => {
                // Check void function return validation
                if let Some(ref return_type) = self.current_function_return_type {
                    let is_void = return_type == "void" || return_type == "()";

                    if is_void {
                        if let Some(ref ret_value) = ret_stmt.value {
                            // Void function has a return value - provide clear error message
                            let func_name = self.current_function_name.as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or("<unknown>");

                            return Err(TypeSystemError::ParseError {
                                type_str: return_type.clone(),
                                reason: format!(
                                    "function '{}' is declared to return void, but returns a value: {}",
                                    func_name, ret_value
                                ),
                            });
                        }
                    }
                }

                // Return value is stored as String, not parsed Expression
                // Will be analyzed during code generation
                Ok(())
            }
            crate::parser::Statement::If(_) => {
                // If condition is stored as String, not parsed Expression
                // Will be analyzed during code generation
                Ok(())
            }
            crate::parser::Statement::While(_) => {
                // While condition is stored as String, not parsed Expression
                // Will be analyzed during code generation
                Ok(())
            }
            crate::parser::Statement::Main(_) => {
                // Main entry point - no expression analysis needed
                Ok(())
            }
            _ => {
                // 其他语句类型的简化处理
                Ok(())
            }
        }
    }

    /// Infer the type of an expression
    fn infer_expression_type(&self, expr: &crate::parser::expr::Expression) -> Result<Type, TypeSystemError> {
        use crate::parser::expr::Expression;

        match expr {
            Expression::Literal(lit) => {
                // Infer type from literal
                eprintln!("[DEBUG] infer_expression_type: inferring type for literal '{}'", lit);
                // Check for float first (to handle 0.0, 1.0, etc.)
                if lit.contains('.') || lit.contains('e') || lit.contains('E') {
                    if lit.parse::<f64>().is_ok() {
                        eprintln!("[DEBUG] infer_expression_type: '{}' is a float literal", lit);
                        return Ok(Type::float());
                    }
                }
                
                if lit == "true" || lit == "false" {
                    eprintln!("[DEBUG] infer_expression_type: '{}' is a boolean literal", lit);
                    Ok(Type::bool())
                } else if lit.parse::<i64>().is_ok() {
                    eprintln!("[DEBUG] infer_expression_type: '{}' is an integer literal", lit);
                    Ok(Type::int())
                } else if lit.parse::<f64>().is_ok() {
                    eprintln!("[DEBUG] infer_expression_type: '{}' is a float literal (fallback)", lit);
                    Ok(Type::float())
                } else if lit.starts_with('"') || lit.starts_with('\'') {
                    eprintln!("[DEBUG] infer_expression_type: '{}' is a string literal", lit);
                    Ok(Type::string())
                } else {
                    eprintln!("[DEBUG] infer_expression_type: '{}' is unknown, defaulting to int", lit);
                    // Unknown literal type, default to int
                    Ok(Type::int())
                }
            }
            Expression::Variable(name) => {
                // Get type from symbol table
                let symbols = self.symbol_space.read()
                    .map_err(|_| TypeSystemError::ParseError {
                        type_str: name.clone(),
                        reason: "Failed to access symbol table".to_string(),
                    })?;

                // Try to find the variable in the symbol table
                // First try the simple name, then try qualified names (function::variable)
                let binding = if let Some(b) = symbols.lookup(name) {
                    Some(b)
                } else if let Some(ref func_name) = self.current_function_name {
                    // Try qualified name (function::variable)
                    let qualified_name = format!("{}::{}", func_name, name);
                    symbols.lookup(&qualified_name)
                } else {
                    None
                };

                if let Some(binding) = binding {
                    match &binding.entity {
                        crate::types::definition::Entity::Variable { ty, .. } => Ok(ty.clone()),
                        crate::types::definition::Entity::Function { .. } => {
                            // Functions themselves are not values, but can be called
                            Err(TypeSystemError::ParseError {
                                type_str: name.clone(),
                                reason: "Cannot use function as value".to_string(),
                            })
                        }
                        _ => Ok(Type::int()), // Default fallback
                    }
                } else {
                    Err(TypeSystemError::UndefinedVariable {
                        name: name.clone(),
                        span: crate::types::definition::Span::new(0, name.len()),
                    })
                }
            }
            Expression::FString { .. } => Ok(Type::string()),
            Expression::StructLiteral { struct_name, .. } => {
                Ok(Type::NamedType { name: struct_name.clone() })
            }
            Expression::Binary { .. } => Ok(Type::int()), // Simplified
            Expression::Unary { op: _, operand } => {
                // Infer type from operand
                self.infer_expression_type(operand)
            }
            Expression::Call { function, .. } => {
                // For now, we can't infer return types without more info
                // Just return int as fallback
                if let Expression::Variable(func_name) = function.as_ref() {
                    if self.is_c_import(func_name) {
                        return Ok(Type::int()); // C functions default to int
                    }
                }
                Ok(Type::int())
            }
            Expression::Member { .. } => Ok(Type::int()), // Simplified
            Expression::ConstructorCall { class_name, .. } => {
                Ok(Type::NamedType { name: class_name.clone() })
            }
            Expression::Index { .. } => Ok(Type::int()), // Simplified
            Expression::ArrayLiteral { elements } => {
                // Infer type from first element
                if elements.is_empty() {
                    Ok(Type::Array { elem: Box::new(Type::int()), size: 0 })
                } else {
                    let first_type = self.infer_expression_type(&elements[0])?;
                    Ok(Type::Array { elem: Box::new(first_type), size: elements.len() })
                }
            }
            Expression::TupleLiteral { elements } => {
                // Infer types from all elements
                let mut elem_types = Vec::new();
                for element in elements {
                    let elem_type = self.infer_expression_type(element)?;
                    elem_types.push(elem_type);
                }
                Ok(Type::Tuple(elem_types))
            }
            Expression::Assign { .. } => Ok(Type::void()), // Assign expressions don't produce a value
            Expression::TypeCast { target_type, .. } => {
                // Return the target type
                match target_type.as_str() {
                    "int" => Ok(Type::int()),
                    "float" => Ok(Type::float()),
                    "bool" => Ok(Type::bool()),
                    _ => Ok(Type::int()), // Fallback
                }
            }
        }
    }

    /// Analyze an expression and check for undefined symbols
    fn analyze_expression(&self, expr: &crate::parser::expr::Expression) -> Result<(), TypeSystemError> {
        use crate::parser::expr::Expression;

        match expr {
            Expression::Literal(_) => Ok(()),
            Expression::Variable(name) => {
                // Check if it's an enum variant: Enum::Variant or Enum::Variant(args)
                if name.contains("::") {
                    let parts: Vec<&str> = name.split("::").collect();
                    if parts.len() == 2 {
                        let enum_name = parts[0].trim();
                        let variant_part = parts[1].trim();
                        
                        // Check if this is a variant with parameters
                        let variant_name = if variant_part.contains('(') {
                            let paren_pos = variant_part.find('(').unwrap();
                            &variant_part[..paren_pos]
                        } else {
                            variant_part
                        };
                        
                        // Try to resolve enum type
                        match self.type_registry.read() {
                            Ok(reg) => {
                                if let Some(TypeDef::Enum { variants, .. }) = reg.get_type(enum_name) {
                                    // Check if this variant exists
                                    if variants.iter().any(|v| v.name == variant_name) {
                                        // Enum variant exists - it's valid
                                        return Ok(());
                                    }
                                }
                            },
                            Err(_) => {}
                        }
                    }
                }
                
                // Check if it's a C library import - these are external symbols
                if self.is_c_import(name) {
                    return Ok(());
                }

                // Check if it's a command-line argument (arg1, arg2, etc.)
                if self.is_command_line_arg(name) {
                    return Ok(());
                }

                // Check if variable or function exists in symbol space
                let symbols = self.symbol_space.read()
                    .map_err(|_| TypeSystemError::ParseError {
                        type_str: name.clone(),
                        reason: "Failed to access symbol table".to_string(),
                    })?;

                // First, try to find the symbol directly
                if symbols.lookup(name).is_some() {
                    return Ok(());
                }

                // If not found, try to find it in the current function's scope
                // by checking for function::parameter pattern
                if let Some(ref func_name) = self.current_function_name {
                    let qualified_name = format!("{}::{}", func_name, name);
                    if symbols.lookup(&qualified_name).is_some() {
                        return Ok(());
                    }
                }

                // Build a detailed error message
                let mut error_msg = format!("undefined symbol '{}\n  = note: the symbol '{} is not defined in the current scope
  = help: check the symbol name or import it using the 'use' statement", name, name);

                // Try to find similar symbols for suggestions
                let all_symbols: Vec<String> = symbols.declared_symbols()
                    .iter()
                    .filter(|s| !s.starts_with('_')) // Skip private symbols
                    .cloned()
                    .collect();

                // Find the best similar symbol using a scoring system
                // Score = Levenshtein distance + length difference penalty
                // This favors symbols with similar lengths
                let mut best_match: Option<String> = None;
                let mut best_score = f64::MAX;

                for symbol in &all_symbols {
                    let distance = levenshtein_distance(name, symbol);
                    if distance <= 3 {
                        // Add a penalty for length difference to prefer symbols of similar length
                        let length_diff = (name.len() as i64 - symbol.len() as i64).abs() as f64;
                        let score = distance as f64 + (length_diff * 0.5);

                        if score < best_score {
                            best_score = score;
                            best_match = Some(symbol.clone());
                        }
                    }
                }

                // Add hint if we found a similar symbol
                if let Some(symbol) = best_match {
                    error_msg.push_str(&format!("\n  hint: did you mean '{}'?", symbol));
                }

                // Check if symbol exists in available symbols list
                if !all_symbols.is_empty() {
                    error_msg.push_str(&format!("\n  note: available symbols in this scope: {}",
                        all_symbols.iter()
                            .take(5)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")));
                }

                Err(TypeSystemError::ParseError {
                    type_str: name.clone(),
                    reason: error_msg,
                })
            }
            Expression::FString { template: _, placeholders } => {
                // Check all placeholder variables
                for placeholder in placeholders {
                    self.analyze_expression(&Expression::Variable(placeholder.clone()))?;
                }
                Ok(())
            }
            Expression::StructLiteral { struct_name, fields } => {
                // Check that struct exists
                let symbols = self.symbol_space.read()
                    .map_err(|_| TypeSystemError::ParseError {
                        type_str: struct_name.clone(),
                        reason: "Failed to access symbol table".to_string(),
                    })?;

                if symbols.lookup(struct_name).is_none() {
                    return Err(TypeSystemError::ParseError {
                        type_str: struct_name.clone(),
                        reason: format!("undefined struct '{}'", struct_name),
                    });
                }

                // Check field expressions
                for (_, field_value) in fields {
                    self.analyze_expression(field_value)?;
                }
                Ok(())
            }
            Expression::Binary { left, op: _, right } => {
                self.analyze_expression(left)?;
                self.analyze_expression(right)
            }
            Expression::Unary { op: _, operand } => {
                self.analyze_expression(operand)
            }
            Expression::Call { function, args } => {
                // Check if this is an enum variant: Enum::Variant(args)
                let is_enum_variant = if let Expression::Variable(func_name) = function.as_ref() {
                    if func_name.contains("::") {
                        let parts: Vec<&str> = func_name.split("::").collect();
                        if parts.len() == 2 {
                            let enum_name = parts[0].trim();
                            let variant_name = parts[1].trim();
                            
                            eprintln!("[DEBUG] analyze_expression: checking enum variant '{}::{}'", enum_name, variant_name);
                            
                            // Check if this is a variant with parameters
                            let variant_name_clean = if variant_name.contains('(') {
                                let paren_pos = variant_name.find('(').unwrap();
                                &variant_name[..paren_pos]
                            } else {
                                variant_name
                            };
                            
                            eprintln!("[DEBUG] analyze_expression: variant_name_clean = '{}'", variant_name_clean);
                            
                            // Try to resolve enum type
                            match self.type_registry.read() {
                                Ok(reg) => {
                                    eprintln!("[DEBUG] analyze_expression: checking type registry for enum '{}'", enum_name);
                                    if let Some(TypeDef::Enum { variants, .. }) = reg.get_type(enum_name) {
                                        eprintln!("[DEBUG] analyze_expression: found enum '{}' with {} variants", enum_name, variants.len());
                                        // Check if this variant exists
                                        if variants.iter().any(|v| v.name == variant_name_clean) {
                                            eprintln!("[DEBUG] analyze_expression: found variant '{}' in enum '{}'", variant_name_clean, enum_name);
                                            // Enum variant exists - just analyze arguments
                                            for arg in args {
                                                self.analyze_expression(arg)?;
                                            }
                                            return Ok(());
                                        } else {
                                            eprintln!("[DEBUG] analyze_expression: variant '{}' NOT found in enum '{}', available variants: {:?}", variant_name_clean, enum_name, variants.iter().map(|v| &v.name).collect::<Vec<_>>());
                                        }
                                    } else {
                                        eprintln!("[DEBUG] analyze_expression: enum '{}' NOT found in type registry", enum_name);
                                    }
                                },
                                Err(e) => {
                                    eprintln!("[DEBUG] analyze_expression: failed to access type registry: {:?}", e);
                                }
                            }
                        }
                    }
                    false
                } else {
                    false
                };
                
                // First, analyze the function expression
                self.analyze_expression(function)?;

                // Then, check if it's actually a function call
                // If function is a variable, check if it's callable
                if let Expression::Variable(func_name) = function.as_ref() {
                    // Check if it's a C library import
                    if self.is_c_import(func_name) {
                        // Check if we have type information for this C function
                        if let Some(c_symbol) = self.get_c_symbol(func_name) {
                            // Check argument count
                            if !c_symbol.is_variadic && args.len() != c_symbol.parameters.len() {
                                return Err(TypeSystemError::ArityMismatch {
                                    expected: c_symbol.parameters.len(),
                                    found: args.len(),
                                    span: crate::types::definition::Span::new(0, 0),
                                });
                            }

                            // Check argument types against parameter types
                            for (i, arg) in args.iter().enumerate() {
                                // Skip type checking for variadic arguments
                                if c_symbol.is_variadic && i >= c_symbol.parameters.len() {
                                    self.analyze_expression(arg)?;
                                    continue;
                                }

                                eprintln!("[DEBUG] analyze_expression: checking arg {} of type {:?}", i, arg);
                                let arg_type = self.infer_expression_type(arg)?;
                                eprintln!("[DEBUG] analyze_expression: inferred arg type as {:?}", arg_type);
                                let param_type_str = &c_symbol.parameters[i].param_type;

                                // Parse parameter type
                                let param_type = self.type_registry.read()
                                    .map_err(|_| TypeSystemError::ParseError {
                                        type_str: param_type_str.clone(),
                                        reason: "Failed to access type registry".to_string(),
                                    })?
                                    .resolve_type(param_type_str)?;

                                // Check for type mismatch
                                // Special case: allow int -> int (regardless of bit width)
                                let types_match = match (&arg_type, &param_type) {
                                    (Type::Int { .. }, Type::Int { .. }) => {
                                        // Allow any int to any int (will be truncated/extended)
                                        true
                                    }
                                    // Otherwise require exact match
                                    _ => arg_type == param_type,
                                };

                                eprintln!("[DEBUG] analyze_expression: checking arg {} type: {:?} vs {:?}", i, arg_type, param_type);
                                eprintln!("[DEBUG] analyze_expression: types_match = {}", types_match);

                                if !types_match {
                                    eprintln!("[DEBUG] analyze_expression: creating TypeMismatch error");
                                    return Err(TypeSystemError::TypeMismatch {
                                        expected: param_type,
                                        found: arg_type,
                                        span: crate::types::definition::Span::new(0, 0),
                                    });
                                }
                            }

                            return Ok(());
                        } else {
                            // No type information available, just analyze arguments
                            for arg in args {
                                self.analyze_expression(arg)?;
                            }
                            return Ok(());
                        }
                    }

                    let symbols = self.symbol_space.read()
                        .map_err(|_| TypeSystemError::ParseError {
                            type_str: func_name.clone(),
                            reason: "Failed to access symbol table".to_string(),
                        })?;

                    if let Some(binding) = symbols.lookup(func_name) {
                        // Check if it's a function
                        if let crate::types::definition::Entity::Function { params, .. } = &binding.entity {
                            // Check argument count
                            if args.len() != params.len() {
                                return Err(TypeSystemError::ArityMismatch {
                                    expected: params.len(),
                                    found: args.len(),
                                    span: crate::types::definition::Span::new(0, 0),
                                });
                            }

                            // Check argument types against parameter types
                            for (i, arg) in args.iter().enumerate() {
                                let arg_type = self.infer_expression_type(arg)?;
                                let param_type = &params[i];

                                // Check for type mismatch
                                // Special case: allow int -> int (regardless of bit width)
                                let types_match = match (&arg_type, param_type) {
                                    (Type::Int { .. }, Type::Int { .. }) => {
                                        // Allow any int to any int (will be truncated/extended)
                                        true
                                    }
                                    // Otherwise require exact match
                                    _ => arg_type == *param_type,
                                };

                                if !types_match {
                                    return Err(TypeSystemError::TypeMismatch {
                                        expected: param_type.clone(),
                                        found: arg_type,
                                        span: crate::types::definition::Span::new(0, 0),
                                    });
                                }
                            }

                            Ok(())
                        } else {
                            // Not a function
                            Err(TypeSystemError::NotCallable {
                                ty: Type::int(), // Simplified
                                span: crate::types::definition::Span::new(0, 0),
                            })
                        }
                    } else {
                        Err(TypeSystemError::NotFound {
                            name: func_name.clone(),
                            kind: crate::types::definition::SpaceKind::Symbol,
                        })
                    }
                } else {
                    // Function expression is not a simple variable, analyze arguments
                    for arg in args {
                        self.analyze_expression(arg)?;
                    }
                    Ok(())
                }
            }
            Expression::Member { object, field, args } => {
                self.analyze_expression(object)?;
                
                // If args is not empty, this is a method call
                if !args.is_empty() {
                    // Check argument expressions
                    for arg in args {
                        let arg_expr = crate::parser::expr::parse_expression(arg)
                            .map_err(|e| TypeSystemError::ParseError {
                                type_str: "method argument".to_string(),
                                reason: e,
                            })?;
                        self.analyze_expression(&arg_expr)?;
                    }
                }
                Ok(())
            }
            Expression::ConstructorCall { class_name, args } => {
                // Check that class exists
                let symbols = self.symbol_space.read()
                    .map_err(|_| TypeSystemError::ParseError {
                        type_str: class_name.clone(),
                        reason: "Failed to access symbol table".to_string(),
                    })?;

                if symbols.lookup(class_name).is_none() {
                    return Err(TypeSystemError::ParseError {
                        type_str: class_name.clone(),
                        reason: format!("undefined class '{}'", class_name),
                    });
                }

                // Check argument expressions
                for arg in args {
                    let arg_expr = crate::parser::expr::parse_expression(arg)
                        .map_err(|e| TypeSystemError::ParseError {
                            type_str: "constructor argument".to_string(),
                            reason: e,
                        })?;
                    self.analyze_expression(&arg_expr)?;
                }
                Ok(())
            }
            Expression::Index { array, index } => {
                self.analyze_expression(array)?;
                self.analyze_expression(index)
            }
            Expression::Assign { object, field_name, value } => {
                // Check that object is 'self'
                match object.as_ref() {
                    Expression::Variable(name) if name == "self" => {
                        // Check the value expression
                        self.analyze_expression(value)
                    }
                    _ => Err(TypeSystemError::ParseError {
                        type_str: "assign".to_string(),
                        reason: format!("assign can only be used with 'self', found {}", object),
                    }),
                }
            }
            Expression::ArrayLiteral { elements } => {
                // Check all element expressions
                for element in elements {
                    self.analyze_expression(element)?;
                }
                Ok(())
            }
            Expression::TupleLiteral { elements } => {
                // Check all element expressions
                for element in elements {
                    self.analyze_expression(element)?;
                }
                Ok(())
            }
            Expression::TypeCast { value, .. } => {
                // Check the value expression
                self.analyze_expression(value)
            }
        }
    }

    /// 获取分析报告
    pub fn report(&self) -> AnalysisReport {
        let scope_info = if let Ok(scope) = self.scope_space.read() {
            ScopeInfo {
                total_scopes: 1 + scope.children().len(), // 简化计算
                bindings: scope.all_names().len(),
                depth: scope.depth(),
            }
        } else {
            ScopeInfo::default()
        };

        let symbol_info = if let Ok(symbols) = self.symbol_space.read() {
            SymbolInfo {
                declared: symbols.declared_symbols().len(),
                referenced: symbols.referenced_symbols().len(),
                unused: symbols.unused_symbols(),
            }
        } else {
            SymbolInfo::default()
        };

        AnalysisReport {
            scope_info,
            symbol_info,
            errors: self.errors.clone(),
        }
    }

    /// Generate a report with source file information (for compatibility)
    pub fn to_report_with_source(&self, _source_file: Option<String>) -> AnalysisReport {
        // For now, just use the same report generation, but could include source file info in the future
        self.report()
    }

    //=============================================================================
    // Constant Folding and Compile-Time Evaluation
    //=============================================================================

    /// Evaluate a constant expression at compile time
    /// Returns Some(value) if the expression can be evaluated as a constant,
    /// None if it cannot be evaluated at compile time
    pub fn eval_constant_expr(&self, expr: &str) -> Option<ConstantValue> {
        let expr = expr.trim();

        // Integer literal
        if let Ok(i) = expr.parse::<i64>() {
            return Some(ConstantValue::Int(i));
        }

        // Float literal
        if let Ok(f) = expr.parse::<f64>() {
            return Some(ConstantValue::Float(f));
        }

        // Boolean literal
        match expr {
            "true" => return Some(ConstantValue::Bool(true)),
            "false" => return Some(ConstantValue::Bool(false)),
            _ => {}
        }

        // String literal
        if expr.starts_with('"') && expr.ends_with('"') {
            let content = &expr[1..expr.len()-1];
            return Some(ConstantValue::String(content.to_string()));
        }

        // Try binary operations with constant folding
        for op in ["+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=", "&&", "||"] {
            if let Some(pos) = expr.find(op) {
                if pos > 0 {
                    let left_str = &expr[..pos].trim();
                    let right_str = &expr[pos + op.len()..].trim();

                    // Try to evaluate both sides recursively
                    if let Some(left) = self.eval_constant_expr(left_str) {
                        if let Some(right) = self.eval_constant_expr(right_str) {
                            return self.eval_constant_binary_op(op, &left, &right).ok();
                        }
                    }
                }
            }
        }

        // Unary operations
        if expr.starts_with('-') {
            let operand_str = &expr[1..].trim();
            if let Some(operand) = self.eval_constant_expr(operand_str) {
                return self.eval_constant_unary_op("-", &operand).ok();
            }
        }

        if expr.starts_with('!') {
            let operand_str = &expr[1..].trim();
            if let Some(operand) = self.eval_constant_expr(operand_str) {
                return self.eval_constant_unary_op("!", &operand).ok();
            }
        }

        // Cannot evaluate as constant
        None
    }

    /// Evaluate binary operation on constants (internal)
    fn eval_constant_binary_op(
        &self,
        op: &str,
        left: &super::super::semantic::analyzer::ConstantValue,
        right: &super::super::semantic::analyzer::ConstantValue,
    ) -> Result<super::super::semantic::analyzer::ConstantValue, String> {
        match (left, right) {
            (super::super::semantic::analyzer::ConstantValue::Int(l), super::super::semantic::analyzer::ConstantValue::Int(r)) => {
                let result = match op {
                    "+" => l.checked_add(*r)
                        .ok_or_else(|| "Integer overflow in constant expression: addition".to_string())?,
                    "-" => l.checked_sub(*r)
                        .ok_or_else(|| "Integer overflow in constant expression: subtraction".to_string())?,
                    "*" => l.checked_mul(*r)
                        .ok_or_else(|| "Integer overflow in constant expression: multiplication".to_string())?,
                    "/" => {
                        if *r == 0 {
                            return Err("Division by zero in constant expression".to_string());
                        }
                        l / r
                    }
                    "%" => {
                        if *r == 0 {
                            return Err("Modulo by zero in constant expression".to_string());
                        }
                        l % r
                    }
                    "==" => return Ok(ConstantValue::Bool(l == r)),
                    "!=" => return Ok(ConstantValue::Bool(l != r)),
                    "<" => return Ok(ConstantValue::Bool(l < r)),
                    ">" => return Ok(ConstantValue::Bool(l > r)),
                    "<=" => return Ok(ConstantValue::Bool(l <= r)),
                    ">=" => return Ok(ConstantValue::Bool(l >= r)),
                    "&&" => return Ok(ConstantValue::Bool(*l != 0 && *r != 0)),
                    "||" => return Ok(ConstantValue::Bool(*l != 0 || *r != 0)),
                    _ => return Err(format!("Unsupported operator for integers: {}", op)),
                };
                Ok(ConstantValue::Int(result))
            }
            (ConstantValue::Float(l), ConstantValue::Float(r)) => {
                let result = match op {
                    "+" => {
                        let res = l + r;
                        // MEDIUM-2 FIX: Check for overflow/underflow in float addition
                        if res.is_infinite() && !l.is_infinite() && !r.is_infinite() {
                            return Err("Float overflow in constant expression: addition".to_string());
                        }
                        if res.is_nan() {
                            return Err("Float operation produced NaN in constant expression".to_string());
                        }
                        res
                    }
                    "-" => {
                        let res = l - r;
                        // MEDIUM-2 FIX: Check for overflow/underflow in float subtraction
                        if res.is_infinite() && !l.is_infinite() && !r.is_infinite() {
                            return Err("Float overflow in constant expression: subtraction".to_string());
                        }
                        if res.is_nan() {
                            return Err("Float operation produced NaN in constant expression".to_string());
                        }
                        res
                    }
                    "*" => {
                        let res = l * r;
                        // MEDIUM-2 FIX: Check for overflow in float multiplication
                        if res.is_infinite() && !l.is_infinite() && !r.is_infinite() {
                            return Err("Float overflow in constant expression: multiplication".to_string());
                        }
                        if res.is_nan() {
                            return Err("Float operation produced NaN in constant expression".to_string());
                        }
                        res
                    }
                    "/" => {
                        // MEDIUM-2 FIX: Check for division by zero in constant folding
                        if *r == 0.0 {
                            return Err("Division by zero in constant expression".to_string());
                        }
                        let res = l / r;
                        // Check for overflow (can happen with very small divisors)
                        if res.is_infinite() && !l.is_infinite() {
                            return Err("Float overflow in constant expression: division".to_string());
                        }
                        if res.is_nan() {
                            return Err("Float operation produced NaN in constant expression".to_string());
                        }
                        res
                    }
                    "==" => return Ok(ConstantValue::Bool((l - r).abs() < f64::EPSILON)),
                    "!=" => return Ok(ConstantValue::Bool((l - r).abs() >= f64::EPSILON)),
                    "<" => return Ok(ConstantValue::Bool(l < r)),
                    ">" => return Ok(ConstantValue::Bool(l > r)),
                    "<=" => return Ok(ConstantValue::Bool(l <= r)),
                    ">=" => return Ok(ConstantValue::Bool(l >= r)),
                    "&&" => return Ok(ConstantValue::Bool(*l != 0.0 && *r != 0.0)),
                    "||" => return Ok(ConstantValue::Bool(*l != 0.0 || *r != 0.0)),
                    _ => return Err(format!("Unsupported operator for floats: {}", op)),
                };
                Ok(ConstantValue::Float(result))
            }
            (ConstantValue::Bool(l), ConstantValue::Bool(r)) => {
                match op {
                    "==" => Ok(ConstantValue::Bool(l == r)),
                    "!=" => Ok(ConstantValue::Bool(l != r)),
                    "&&" => Ok(ConstantValue::Bool(*l && *r)),
                    "||" => Ok(ConstantValue::Bool(*l || *r)),
                    _ => Err(format!("Unsupported operator for booleans: {}", op)),
                }
            }
            _ => Err(format!("Type mismatch in constant expression: {} and {}",
                left.type_name(), right.type_name())),
        }
    }

    /// Evaluate unary operation on constant (internal)
    fn eval_constant_unary_op(
        &self,
        op: &str,
        operand: &ConstantValue,
    ) -> Result<ConstantValue, String> {
        match operand {
            ConstantValue::Int(i) => {
                match op {
                    "-" => {
                        if *i == i64::MIN {
                            // Negation overflow - return safe default (0)
                            Ok(ConstantValue::Int(0))
                        } else {
                            Ok(ConstantValue::Int(-i))
                        }
                    }
                    "!" => Ok(ConstantValue::Bool(*i == 0)),
                    _ => Err(format!("Unsupported unary operator for int: {}", op)),
                }
            }
            ConstantValue::Float(f) => {
                match op {
                    "-" => Ok(ConstantValue::Float(-f)),
                    "!" => Ok(ConstantValue::Bool(*f == 0.0)),
                    _ => Err(format!("Unsupported unary operator for float: {}", op)),
                }
            }
            ConstantValue::Bool(b) => {
                match op {
                    "!" => Ok(ConstantValue::Bool(!b)),
                    _ => Err(format!("Unsupported unary operator for bool: {}", op)),
                }
            }
            _ => Err(format!("Cannot apply unary operator to type: {}", operand.type_name())),
        }
    }

    //=============================================================================
    // Enhanced Safety Analysis
    //=============================================================================

    /// Check if code is reachable (detects dead code patterns)
    fn is_code_reachable(&self, code: &str) -> bool {
        let code = code.trim();

        // Check for obviously false conditions
        if code.starts_with("if false") || code.starts_with("if !true") {
            return false;
        }

        // Check for conditions that will never be true
        if let Some(const_val) = self.eval_constant_expr(code) {
            if let ConstantValue::Bool(false) = const_val {
                return false;
            }
        }

        true
    }

    /// Extract break statements from reachable code only
    fn extract_reachable_breaks(&self, code: &str) -> bool {
        let mut has_reachable_break = false;
        let mut depth = 0; // Track nesting depth (if, while, etc.)
        let mut in_unreachable = false;

        for line in code.lines() {
            let trimmed = line.trim();

            // Track nesting
            if trimmed.starts_with("if ") || trimmed.starts_with("while ") ||
               trimmed.starts_with("for ") || trimmed.starts_with("match ") {
                depth += 1;

                // Check if this block is unreachable
                if trimmed.starts_with("if false") || trimmed.starts_with("if !true") {
                    in_unreachable = true;
                }
            }

            // Count closing braces to track when we exit blocks
            depth -= trimmed.matches('}').count();

            // Skip unreachable code
            if in_unreachable {
                if trimmed == "}" && depth == 0 {
                    in_unreachable = false;
                }
                continue;
            }

            // Check for break in reachable code
            if trimmed == "break" || trimmed.starts_with("break ") {
                has_reachable_break = true;
                break;
            }
        }

        has_reachable_break
    }

    /// Detect potential infinite loops with control flow analysis
    pub fn detect_infinite_loop(&self, condition: &str, body: &str) -> Option<String> {
        let condition = condition.trim();

        // Check for obviously always-true conditions
        let is_always_true = match condition {
            "true" | "1" => true,
            cond => {
                if let Some(const_val) = self.eval_constant_expr(cond) {
                    matches!(const_val, ConstantValue::Int(i) if i != 0)
                        || matches!(const_val, ConstantValue::Bool(true))
                } else {
                    false
                }
            }
        };

        if is_always_true {
            // Check if there's a reachable break statement
            let has_reachable_break = self.extract_reachable_breaks(body);

            if !has_reachable_break {
                return Some(
                    format!("Infinite loop detected: 'while {}' has no reachable break statement", condition)
                );
            }
        }

        None
    }

    /// Check for unused variables
    pub fn check_unused_variables(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Get the symbol space
        let symbol_space = self.symbol_space.read().unwrap();
        let unused = symbol_space.unused_symbols();

        for var_name in unused {
            // Skip common special variables
            if var_name.starts_with('_') || var_name == "main" {
                continue;
            }

            // Skip command-line arguments (arg1, arg2, etc.)
            if self.is_command_line_arg(&var_name) {
                continue;
            }

            warnings.push(format!("Warning: Variable '{}' declared but never used", var_name));
        }

        warnings
    }

    /// Check for dead code after return statements
    pub fn check_dead_code_after_return(&self, statements: &[String]) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut found_return = false;

        for stmt in statements {
            if found_return {
                warnings.push(format!("Unreachable code after return statement: {}", stmt));
            }

            if stmt.contains("return ") || stmt.starts_with("return") {
                found_return = true;
            }
        }

        warnings
    }

    /// Extract while loop bodies from code for analysis
    fn analyze_while_loops(&self, code: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = code.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Detect while loop start
            if line.starts_with("while ") {
                // Extract condition
                let condition = if let Some(start) = line.find("while ") {
                    let rest = &line[start + 6..];
                    if let Some(end) = rest.find(':') {
                        rest[..end].trim()
                    } else {
                        rest.trim()
                    }
                } else {
                    i += 1;
                    continue;
                };

                // Collect loop body
                let mut body = String::new();
                let mut brace_count = 0;
                let started_at = i;
                i += 1;

                // Safety check: make sure we didn't go out of bounds
                if i >= lines.len() {
                    break;
                }

                while i < lines.len() {
                    let body_line = lines[i].trim();

                    // Track braces for nested blocks
                    brace_count += body_line.matches('{').count();
                    brace_count -= body_line.matches('}').count();

                    if !body_line.is_empty() {
                        body.push_str(body_line);
                        body.push('\n');
                    }

                    // Check if we've exited the loop (same indentation or closing brace)
                    if !body.is_empty() && brace_count <= 0 {
                        if body_line.starts_with("}") || i > started_at + 1 {
                            break;
                        }
                    }

                    i += 1;

                    // Prevent infinite loops in parsing
                    if i > started_at + 1000 {
                        break;
                    }
                }

                // Analyze this while loop
                if let Some(warning) = self.detect_infinite_loop(condition, &body) {
                    warnings.push(warning);
                }
            }

            i += 1;
        }

        warnings
    }

    /// Perform comprehensive safety analysis on a code block
    pub fn analyze_safety(&self, code: &str) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check for unused variables
        warnings.extend(self.check_unused_variables());

        // Analyze while loops for infinite loops with control flow
        warnings.extend(self.analyze_while_loops(code));

        // Check for suspicious comparisons
        if code.contains("== ") || code.contains("!= ") {
            if code.contains("true") || code.contains("false") {
                warnings.push("Style: Direct comparison with boolean - use the value directly".to_string());
            }
        }

        // Check for dead code after returns (simple line-based check)
        let lines: Vec<String> = code.lines().map(|l| l.to_string()).collect();
        warnings.extend(self.check_dead_code_after_return(&lines));

        warnings
    }
}

/// 分析报告
#[derive(Debug, Clone)]
pub struct AnalysisReport {
    pub scope_info: ScopeInfo,
    pub symbol_info: SymbolInfo,
    pub errors: Vec<TypeSystemError>,
}

/// 作用域信息
#[derive(Debug, Clone, Default)]
pub struct ScopeInfo {
    pub total_scopes: usize,
    pub bindings: usize,
    pub depth: usize,
}

/// 符号信息
#[derive(Debug, Clone, Default)]
pub struct SymbolInfo {
    pub declared: usize,
    pub referenced: usize,
    pub unused: Vec<String>,
}