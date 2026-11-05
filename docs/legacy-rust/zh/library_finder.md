# 库查找器模块

## 概述

库查找器模块（`src/library_finder.rs`）为静态和动态库提供统一的库搜索功能。该模块通过提供集中的库文件发现机制，消除了 `main.rs` 和 `linker.rs` 之间的代码重复。

## 模块目的

库查找器服务于几个关键目的：

1. **统一搜索**：查找静态（`.a`）和动态（`.so`）库的单一接口
2. **标准路径**：在标准系统位置搜索
3. **优先级顺序**：按定义的优先级顺序搜索位置
4. **跨平台**：在不同的类 Unix 系统上工作
5. **代码重用**：消除编译器组件之间的重复

## 核心函数

### find_library_file

在标准位置查找库文件：

```rust
pub fn find_library_file(lib_name: &str) -> Option<String>
```

**参数：**
- `lib_name`：库名称（例如，"m"、"pthread"、"mylib"）

**返回：**
- `Some(String)`：如果找到，返回库文件的完整路径
- `None`：如果在任何搜索位置都未找到库文件

**搜索位置（按优先级顺序）：**
1. 当前目录（`.`）
2. `./lib` 子目录
3. `$PATH` 环境变量中的目录
4. `/usr/lib`
5. `/usr/local/lib`
6. `/lib`
7. `/lib64`
8. `~/.local/lib`

**库名称处理：**

该函数处理两种库名称格式：

```rust
// 带 "lib" 前缀
find_library_file("libc")   // 搜索 libc.so 或 libc.a

// 不带 "lib" 前缀
find_library_file("m")      // 搜索 libm.so 或 libm.a
```

**文件扩展名：**

该函数搜索动态和静态库：

```rust
// 动态库
libm.so
libc.so

// 静态库
libm.a
libc.a
```

### get_standard_search_paths

返回标准库搜索路径：

```rust
pub fn get_standard_search_paths() -> Vec<String>
```

**返回：**
用于搜索库文件的目录路径向量

**默认搜索路径：**

```rust
vec![
    ".",                       // 当前目录
    "./lib",                   // 本地 lib 目录
    // PATH 目录（动态）
    "/usr/lib",                // 系统库
    "/usr/local/lib",          // 本地系统库
    "/lib",                    // 基本库
    "/lib64",                  // 64 位库
    "~/.local/lib",            // 用户本地库
]
```

## 实现细节

### 搜索算法

库查找器使用以下搜索算法：

```rust
pub fn find_library_file(lib_name: &str) -> Option<String> {
    // 1. 生成可能的库文件名
    let lib_filenames = if lib_name.starts_with("lib") {
        vec![
            format!("{}.so", lib_name),  // 动态
            format!("{}.a", lib_name),   // 静态
        ]
    } else {
        vec![
            format!("lib{}.so", lib_name),  // 动态
            format!("lib{}.a", lib_name),   // 静态
        ]
    };

    // 2. 获取搜索路径
    let search_paths = get_standard_search_paths();

    // 3. 在每个路径中搜索
    for search_path in &search_paths {
        for lib_filename in &lib_filenames {
            let lib_path = Path::new(search_path).join(lib_filename);
            if lib_path.exists() {
                return Some(lib_path.to_string_lossy().to_string());
            }
        }
    }

    // 4. 未找到
    None
}
```

### 路径解析

该函数使用 `Path::join` 进行跨平台路径构造：

```rust
let lib_path = Path::new(search_path).join(lib_filename);
```

这确保了不同操作系统上正确的路径分隔符。

### 存在性检查

该函数使用 `Path::exists()` 检查库文件是否存在：

```rust
if lib_path.exists() {
    return Some(lib_path.to_string_lossy().to_string());
}
```

## 使用示例

### 查找标准库

```rust
// 查找 libm（数学库）
if let Some(path) = find_library_file("m") {
    println!("找到 libm：{}", path);
    // 输出：/usr/lib/libm.so
}

// 查找 libc（C 标准库）
if let Some(path) = find_library_file("c") {
    println!("找到 libc：{}", path);
    // 输出：/usr/lib/libc.so
}

// 查找 libpthread（POSIX 线程）
if let Some(path) = find_library_file("pthread") {
    println!("找到 libpthread：{}", path);
    // 输出：/usr/lib/libpthread.so
}
```

### 查找自定义库

```rust
// 在当前目录查找自定义库
if let Some(path) = find_library_file("mylib") {
    println!("找到 mylib：{}", path);
    // 输出：./libmylib.so 或 ./libmylib.a
}

// 查找带 "lib" 前缀的库
if let Some(path) = find_library_file("libcustom") {
    println!("找到 libcustom：{}", path);
    // 输出：./libcustom.so 或 ./libcustom.a
}
```

### 获取搜索路径

```rust
let paths = get_standard_search_paths();
for path in &paths {
    println!("{}", path);
}
```

### 自定义搜索逻辑

```rust
fn find_with_fallback(lib_name: &str, fallback_paths: &[&str]) -> String {
    // 首先尝试标准路径
    if let Some(path) = find_library_file(lib_name) {
        return path;
    }

    // 尝试回退路径
    for fallback_path in fallback_paths {
        let lib_path = Path::new(fallback_path).join(format!("lib{}.so", lib_name));
        if lib_path.exists() {
            return lib_path.to_string_lossy().to_string();
        }
    }

    panic!("未找到库 '{}'", lib_name);
}
```

## 与编译器的集成

### 链接器集成

链接器使用库查找器定位库：

```rust
use crate::library_finder::find_library_file;

pub fn link(&self, object_files: &[PathBuf], output: &Path) -> Result<(), String> {
    for lib_name in &self.c_libraries {
        if let Some(lib_path) = find_library_file(lib_name) {
            // 将库添加到链接器命令
            linker_cmd.arg(lib_path);
        } else {
            return Err(format!("未找到库 '{}'", lib_name));
        }
    }
    // ...
}
```

### 主函数集成

主函数使用库查找器进行 JIT 模式：

```rust
use crate::library_finder::find_library_file;

// 查找库以进行链接
match find_library_file(lib_name) {
    Some(lib_path) => {
        // 将库路径添加到链接器
        linker_cmd.arg(format!("-L{}", lib_dir));
        linker_cmd.arg(format!("-Wl,-rpath,{}", lib_dir));
    }
    None => {
        // 报告错误
        return Err(format!("未找到库 '{}'", lib_name));
    }
}
```

## 搜索路径优先级

### 优先级顺序

库查找器按以下优先级顺序搜索位置：

1. **当前目录（`.`）**：本地开发的最高优先级
2. **`./lib`**：项目特定库的本地库目录
3. **`$PATH`**：用户定义的搜索路径
4. **`/usr/lib`**：标准系统库
5. **`/usr/local/lib`**：本地系统库（通常用于手动安装的软件）
6. **`/lib`**：基本系统库
7. **`/lib64`**：64 位系统库
8. **`~/.local/lib`**：用户本地库

### 为什么是这个顺序？

1. **本地优先**：允许开发者用本地版本覆盖系统库
2. **项目库**：`./lib` 允许项目特定的库
3. **用户路径**：尊重用户的 PATH 环境变量
4. **系统库**：回退到标准系统位置
5. **用户本地**：允许用户特定的库而无需系统范围的安装

## 平台考虑

### 类 Unix 系统

库查找器设计用于类 Unix 系统（Linux、macOS、BSD）：

- 使用正斜杠（`/`）作为路径分隔符
- 搜索标准 Unix 库目录
- 支持 `.so`（共享对象）和 `.a`（归档）文件

### Android (Termux)

库查找器在 Android Termux 环境中工作：

- 搜索 Termux 特定的库路径
- 尊重 Android 的库结构
- 与 Termux 的包管理器库一起工作

### 跨平台路径处理

该函数使用 `std::path::Path` 进行跨平台兼容性：

```rust
let lib_path = Path::new(search_path).join(lib_filename);
```

这确保了不同平台上正确的路径处理。

## 错误处理

### 未找到

如果未找到库，该函数返回 `None`：

```rust
if let Some(path) = find_library_file("nonexistent") {
    println!("找到：{}", path);
} else {
    println!("未找到库");
}
```

### 错误报告

编译器提供详细的错误消息：

```rust
match find_library_file(lib_name) {
    Some(path) => {
        eprintln!("  = 注释：在 '{}' 找到库 '{}'", path, lib_name);
    }
    None => {
        let error = ErrorKind::LinkError {
            details: format!(
                "未找到库 '{}'\n  = 注释：搜索于：当前目录、./lib、$PATH、/usr/lib、/usr/local/lib、~/.local/lib\n  = 帮助：先编译库或将其安装到标准位置",
                lib_name
            ),
        };
        eprintln!("{}", error.description());
    }
}
```

## 性能考虑

### 早期退出

该函数在找到库后立即返回：

```rust
if lib_path.exists() {
    return Some(lib_path.to_string_lossy().to_string());  // 早期退出
}
```

### 最小文件系统访问

该函数仅检查文件存在性：

```rust
if lib_path.exists() {
    // 找到！
}
```

这比读取文件元数据或内容更快。

### 路径缓存

搜索路径生成一次：

```rust
let search_paths = get_standard_search_paths();
```

在实践中，您可以缓存搜索路径以供重复调用。

## 安全考虑

### 路径验证

库查找器使用 `Path::join` 防止路径遍历：

```rust
let lib_path = Path::new(search_path).join(lib_filename);
```

这确保库名称不能逃离搜索目录。

### 无任意执行

该函数仅查找文件，不执行它们：

```rust
if lib_path.exists() {
    return Some(lib_path.to_string_lossy().to_string());
}
```

调用者负责安全地使用找到的库。

### 相对路径

该函数首先搜索相对路径：

```rust
search_paths.push(".".to_string());
search_paths.push("./lib".to_string());
```

这允许本地开发而无需系统范围的安装。

## 测试

### 单元测试

该模块包括单元测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_standard_search_paths() {
        let paths = get_standard_search_paths();
        assert!(paths.len() > 0);
        assert!(paths.contains(&".".to_string()));
        assert!(paths.contains(&"./lib".to_string()));
    }

    #[test]
    fn test_find_library_file_with_prefix() {
        // 测试查找带 "lib" 前缀的库
        let result = find_library_file("libc");
        // 在真实系统中，这可能会找到 /usr/lib/libc.so
    }

    #[test]
    fn test_find_library_file_without_prefix() {
        // 测试查找不带 "lib" 前缀的库
        let result = find_library_file("m");
        // 在真实系统中，这可能会找到 /usr/lib/libm.so
    }
}
```

## 最佳实践

### 1. 处理未找到

```rust
// 好的：处理未找到的情况
if let Some(path) = find_library_file("mylib") {
    // 使用库
} else {
    // 提供有用的错误消息
    eprintln!("未找到库 'mylib'");
    eprintln!("搜索于：{}", get_standard_search_paths().join(":"));
}
```

### 2. 使用标准名称

```rust
// 好的：使用标准库名称
find_library_file("m")      // 数学库
find_library_file("pthread") // POSIX 线程
find_library_file("dl")     // 动态加载

// 避免：非标准名称
find_library_file("math")   // 应该是 "m"
find_library_file("threads") // 应该是 "pthread"
```

### 3. 优先使用动态库

该函数先搜索 `.so` 再搜索 `.a`：

```rust
// 动态库先搜索
vec![
    format!("{}.so", lib_name),  // 动态
    format!("{}.a", lib_name),   // 静态
]
```

这通常更受青睐，因为二进制文件更小且更容易更新。

### 4. 使用项目特定的库

将项目特定的库放在 `./lib` 中：

```bash
project/
├── src/
├── lib/
│   ├── libmylib.so
│   └── libother.a
└── main.cf
```

## 未来增强

### 计划的功能

1. **自定义搜索路径**
   ```rust
   pub fn find_library_file_with_paths(lib_name: &str, paths: &[PathBuf]) -> Option<String>
   ```

2. **库版本支持**
   ```rust
   pub fn find_library_version(lib_name: &str, version: &str) -> Option<String>
   ```

3. **缓存**
   ```rust
   pub struct CachedLibraryFinder {
       cache: HashMap<String, Option<String>>,
   }
   ```

4. **平台特定路径**
   ```rust
   pub fn get_platform_search_paths() -> Vec<String>
   ```

5. **库元数据**
   ```rust
   pub struct LibraryInfo {
       pub path: String,
       pub is_static: bool,
       pub version: Option<String>,
   }
   ```

## 另请参阅

- [编译器模块](compiler.md) - 编译编排
- [链接器模块](compiler/linker.md) - 链接实现
- [C 集成模块](c_integration.md) - C 库导入
- [主模块](main.rs) - 编译器入口点