# 编译器前端模块

## 概述

编译器前端模块（`src/compiler/`）是 Coffee 编译器的核心编排层。它管理从源代码解析、语义分析、类型检查到导入解析的整个编译流程。该模块协调所有其他编译器组件，并提供统一的编译接口。

## 模块结构

```
src/compiler/
├── mod.rs          # 主编译器前端模块
├── unit.rs         # 编译单元管理
├── graph.rs        # 依赖图构建
├── project.rs      # 项目配置管理
├── entry_point.rs  # 主入口点检测
├── scheduler.rs    # 编译调度
├── scanner.rs      # 源文件扫描
├── linker.rs       # 链接器编排
├── builder.rs      # 项目构建器
├── import_resolver.rs  # 导入解析
├── module_loader.rs    # 模块加载
├── pipeline.rs     # 编译流水线
├── session.rs      # 编译会话管理
└── statistics.rs   # 编译统计
```

## 核心组件

### 1. 编译器配置（`CompilerConfig`）

编译器配置控制编译行为的各个方面：

```rust
pub struct CompilerConfig {
    /// 启用导入解析
    pub enable_imports: bool,
    /// 导入搜索路径
    pub import_paths: Vec<PathBuf>,
    /// 最大导入递归深度
    pub max_import_depth: usize,
    /// 缓存解析的模块
    pub cache_modules: bool,
    /// 交叉编译目标三元组
    pub target_triple: Option<String>,
}
```

**默认导入路径：**
- 当前工作目录
- `examples/`
- `src/`
- `lib/`
- `std/`
- `.`（当前目录）

### 2. 编译结果（`CompilationResult`）

编译操作的结果：

```rust
pub struct CompilationResult {
    /// 成功标志
    pub success: bool,
    /// 解析的程序 AST（包括导入的模块）
    pub program: parser::Program,
    /// 错误
    pub errors: Vec<Diagnostic>,
    /// 警告
    pub warnings: Vec<Diagnostic>,
    /// 提示
    pub hints: Vec<Diagnostic>,
    /// 分析报告
    pub report: AnalysisReport,
    /// 编译统计
    pub stats: CompilationStatistics,
    /// 标准库导出
    pub std_exports: Vec<String>,
    /// 导入的模块路径
    pub imported_modules: Vec<String>,
    /// C 库导入
    pub c_imports: Vec<String>,
    /// C 函数符号表
    pub cfc_symbols: HashMap<String, c::CSymbolTable>,
}
```

### 3. 编译统计（`CompilationStatistics`）

编译过程的详细统计信息：

```rust
pub struct CompilationStatistics {
    pub lines_parsed: usize,              // 解析的行数
    pub statements_parsed: usize,         // 解析的语句数
    pub imports_processed: usize,         // 处理的导入数
    pub modules_loaded: usize,            // 加载的模块数
    pub functions_declared: usize,        // 声明的函数数
    pub classes_declared: usize,          // 声明的类数
    pub enums_declared: usize,            // 声明的枚举数
    pub variables_declared: usize,        // 声明的变量数
    pub assignments_made: usize,          // 赋值数
    pub return_statements: usize,         // 返回语句数
    pub if_expressions: usize,            // if 表达式数
    pub while_loops: usize,               // while 循环数
    pub for_loops: usize,                 // for 循环数
    pub match_expressions: usize,         // match 表达式数
    pub break_statements: usize,          // break 语句数
    pub continue_statements: usize,       // continue 语句数
    pub memory_operations: usize,         // 内存操作数
    pub raise_statements: usize,          // raise 语句数
    pub comments_count: usize,            // 注释数
    pub expression_statements: usize,     // 表达式语句数
}
```

### 4. 编译器前端（`CompilerFrontend`）

主编译器前端编排整个编译过程：

```rust
pub struct CompilerFrontend {
    /// 语义分析器
    analyzer: Arc<RwLock<SemanticAnalyzer>>,
    /// 类型检查器
    type_checker: Arc<RwLock<TypeChecker>>,
    /// 诊断发射器
    emitter: DiagnosticEmitter,
    /// 编译器配置
    config: CompilerConfig,
    /// 模块缓存
    module_cache: Arc<RwLock<HashMap<String, ParsedModule>>>,
    /// 当前导入的模块（用于循环检测）
    import_stack: Arc<RwLock<Vec<String>>>,
    /// C 库导入
    c_imports: Arc<RwLock<Vec<String>>>,
    /// CFC 符号表
    cfc_symbols: Arc<RwLock<HashMap<String, c::CSymbolTable>>>,
}
```

## 编译流水线

编译器前端执行以下流水线：

### 第一阶段：源代码解析
1. 从文件读取源代码
2. 使用完整程序解析器解析
3. 验证导入语句位置（必须在函数之前）
4. 带恢复地收集解析错误

### 第二阶段：导入处理
1. 检测程序中的导入语句
2. 处理每个导入：
   - 解析模块路径
   - 从文件系统加载模块
   - 解析模块内容
   - 缓存解析的模块
   - 检测循环依赖
3. 收集 C 库导入用于链接

### 第三阶段：验证
1. 检查多个 main() 函数
2. 验证导入顺序
3. 检测导入错误，如果关键则停止

### 第四阶段：语义分析
1. 按顺序分析每个语句
2. 构建符号表
3. 跟踪变量生命周期
4. 验证变量使用
5. 收集语义错误

### 第五阶段：类型检查
1. 类型检查变量声明
2. 类型检查内存操作
3. 验证类型一致性
4. 收集类型错误

### 第六阶段：程序扩展
1. 用导入的模块扩展程序
2. 处理完整模块导入
3. 处理选择性导入（`use symbol in module`）
4. 将导入的符号合并到程序中

### 第七阶段：结果生成
1. 收集所有诊断（错误、警告、提示）
2. 生成分析报告
3. 编译统计
4. 返回编译结果

## 导入解析

### 导入类型

Coffee 支持多种导入风格：

#### 1. 简单导入
```coffee
use module_name
```
从 `module_name` 导入所有符号。

#### 2. 别名导入
```coffee
use module_name as alias
```
使用别名导入所有符号。

#### 3. 选择性导入
```coffee
use symbol_name in module_name
```
仅从模块导入特定符号。

#### 4. 带别名的选择性导入
```coffee
use symbol_name in module_name as alias
```
使用别名导入特定符号。

#### 5. C 库导入
```coffee
use function_name in libc of c
```
从 C 库导入 C 函数。

### 导入解析过程

1. **路径解析**：在导入路径中搜索模块
2. **模块加载**：读取并解析模块文件
3. **符号收集**：提取导出的符号
4. **依赖跟踪**：构建依赖图
5. **循环检测**：检测循环依赖
6. **C 库处理**：解析 .cfc 文件以获取 C 函数

### 模块缓存

编译器维护模块缓存以避免重新解析同一模块：

```rust
pub struct ParsedModule {
    /// 模块语句
    statements: Vec<parser::Statement>,
    /// 导出的符号
    pub exports: Vec<String>,
    /// 文件路径
    pub file_path: PathBuf,
}
```

## 错误处理

### 错误类别

编译器前端处理来自多个来源的错误：

1. **解析错误**：解析期间的语法错误
2. **导入错误**：未找到模块、循环导入
3. **语义错误**：未定义的符号、无效使用
4. **类型错误**：类型不匹配、无效操作
5. **C 集成错误**：C 库解析失败

### 错误恢复

编译器实现错误恢复以在错误后继续编译：

- 在语法错误后继续解析
- 在报告前收集多个错误
- 提供有用的错误消息和上下文
- 为常见错误建议修复

### 诊断收集

所有诊断都按严重性分类收集：

- **错误**：阻止编译
- **警告**：不阻止编译但应处理
- **提示**：改进建议
- **注释**：附加信息

## 项目配置

### coffee.toml 格式

```toml
[package]
name = "project_name"
version = "0.1.0"
authors = ["Author Name <email@example.com>"]
description = "Project description"

[dependencies]
std = "0.2.0"

[dependencies.packages]
# coffee_package = "0.1.0"

[dependencies.c_libraries.libm]
name = "m"
headers = ["math.h"]
include_paths = ["/usr/include"]
link_flags = ["-L/usr/lib"]
static_link = false
static_lib_path = ""

[build]
main = "src/main"
target_dir = "target"
src_dir = "src"
have_c = true

[target]
opt_level = 2
triple = "x86_64-unknown-linux-gnu"
```

### 项目构建器

`ProjectBuilder` 管理项目编译：

```rust
pub struct ProjectBuilder {
    config: ProjectConfig,
    entry_point: Option<String>,
    target_triple: Option<String>,
    force_static: bool,
}

impl ProjectBuilder {
    pub fn new(config: ProjectConfig) -> Self;
    pub fn set_custom_entry(&mut self, entry: &str);
    pub fn set_target_triple(&mut self, triple: &str);
    pub fn set_force_static(&mut self, force: bool);
    pub fn compile(&mut self) -> Result<PathBuf, String>;
}
```

## 模块子组件

### 编译单元（`unit.rs`）

表示单个编译单元（源文件）：

```rust
pub struct CompilationUnit {
    pub file_path: PathBuf,
    pub source: String,
    pub statements: Vec<Statement>,
    pub status: CompilationStatus,
}

pub enum CompilationStatus {
    NotStarted,      // 未开始
    Parsing,         // 解析中
    Analyzing,       // 分析中
    TypeChecking,    // 类型检查中
    Completed,       // 已完成
    Failed,          // 失败
}
```

### 依赖图（`graph.rs`）

管理模块依赖：

```rust
pub struct DependencyGraph {
    nodes: HashMap<String, ModuleNode>,
    edges: Vec<DependencyEdge>,
}

pub struct ModuleNode {
    pub name: String,
    pub file_path: PathBuf,
    pub status: CompilationStatus,
    pub dependents: Vec<String>,      // 依赖此模块的模块
    pub dependencies: Vec<String>,    // 此模块依赖的模块
}

pub struct DependencyEdge {
    pub from: String,
    pub to: String,
}
```

### 扫描器（`scanner.rs`）

扫描源目录查找 Coffee 文件：

```rust
pub struct SourceScanner {
    src_dir: PathBuf,
    extensions: Vec<String>,
}

impl SourceScanner {
    pub fn new(src_dir: PathBuf) -> Self;
    pub fn scan(&self) -> Result<Vec<PathBuf>, String>;
    pub fn scan_recursive(&self) -> Result<Vec<PathBuf>, String>;
}
```

### 链接器（`linker.rs`）

编排链接过程：

```rust
pub struct Linker {
    target_triple: Option<String>,
    static_linking: bool,
    c_libraries: Vec<String>,
}

impl Linker {
    pub fn new() -> Self;
    pub fn set_target_triple(&mut self, triple: &str);
    pub fn add_library(&mut self, lib: &str);
    pub fn link(&self, object_files: &[PathBuf], output: &Path) -> Result<(), String>;
}
```

### 导入解析器（`import_resolver.rs`）

解析导入语句：

```rust
pub struct ImportResolver {
    import_paths: Vec<PathBuf>,
    module_cache: Arc<RwLock<HashMap<String, ParsedModule>>>,
}

impl ImportResolver {
    pub fn new(import_paths: Vec<PathBuf>) -> Self;
    pub fn resolve(&self, import: &Import) -> Result<ParsedModule, String>;
    pub fn resolve_path(&self, path: &str) -> Result<PathBuf, String>;
}
```

### 模块加载器（`module_loader.rs`）

加载和解析模块：

```rust
pub struct ModuleLoader {
    cache: Arc<RwLock<HashMap<String, ParsedModule>>>,
    import_stack: Arc<RwLock<Vec<String>>>,
}

impl ModuleLoader {
    pub fn new() -> Self;
    pub fn load(&self, path: &Path) -> Result<ParsedModule, String>;
    pub fn is_loaded(&self, path: &str) -> bool;
}
```

### 流水线（`pipeline.rs`）

管理编译流水线：

```rust
pub struct CompilationPipeline {
    config: CompilerConfig,
    stages: Vec<Box<dyn PipelineStage>>,
}

impl CompilationPipeline {
    pub fn new(config: CompilerConfig) -> Self;
    pub fn add_stage(&mut self, stage: Box<dyn PipelineStage>);
    pub fn execute(&mut self, source: &str) -> Result<CompilationResult, String>;
}
```

### 会话（`session.rs`）

管理编译会话：

```rust
pub struct CompilationSession {
    id: String,
    start_time: std::time::Instant,
    config: CompilerConfig,
    results: Vec<CompilationResult>,
}

impl CompilationSession {
    pub fn new(config: CompilerConfig) -> Self;
    pub fn compile(&mut self, source: &str, file: Option<&str>) -> CompilationResult;
    pub fn get_statistics(&self) -> SessionStatistics;
}
```

### 统计（`statistics.rs`）

提供编译统计：

```rust
pub struct CompilationStatistics {
    // 详见上方结构
}

impl CompilationStatistics {
    pub fn format(&self) -> String;
    pub fn merge(&mut self, other: &CompilationStatistics);
}
```

## 使用示例

### 单文件编译

```rust
use compiler::CompilerFrontend;

let mut frontend = CompilerFrontend::new();
let source = r#"
fn add(a: int, b: int) => int:
    return a + b

main(add(1, 2))
"#;

let result = frontend.compile(source, Some("example.cf"));

if result.success {
    println!("编译成功！");
    println!("声明的函数数：{}", result.stats.functions_declared);
} else {
    for error in &result.errors {
        eprintln!("{}", error.format());
    }
}
```

### 项目编译

```rust
use compiler::{ProjectConfig, ProjectBuilder};

let config = ProjectConfig::from_file("coffee.toml")?;
let mut builder = ProjectBuilder::new(config);
builder.set_custom_entry("src/main.cf");

let executable = builder.compile()?;
println!("可执行文件已创建：{}", executable.display());
```

### 自定义导入路径

```rust
use compiler::{CompilerFrontend, CompilerConfig};

let config = CompilerConfig {
    import_paths: vec![
        PathBuf::from("./my_modules"),
        PathBuf::from("./shared"),
    ],
    ..Default::default()
};

let mut frontend = CompilerFrontend::with_config(config);
let result = frontend.compile(source, Some("main.cf"));
```

## 与其他模块的集成

### 解析器集成

编译器前端使用解析器模块解析源代码：

```rust
let program = parser::parse_program(source)?;
```

### 语义分析集成

编译器前端使用语义分析器：

```rust
let analyzer = SemanticAnalyzer::new(type_registry);
analyzer.analyze_statement(statement)?;
```

### 类型检查集成

编译器前端使用类型检查器：

```rust
let type_checker = TypeChecker::new(type_registry, CheckingMode::Comprehensive);
type_checker.check_variable_decl(decl)?;
```

### C 集成集成

编译器前端管理 C 库导入：

```rust
// 收集 C 导入
let c_imports = frontend.get_c_imports();

// 获取 CFC 符号表
let cfc_symbols = frontend.get_cfc_symbols();
```

### 后端集成

编译器前端将程序提供给后端：

```rust
let result = frontend.compile(source, Some("file.cf"));
backend::compile_program(&result.program, &result.c_imports, result.cfc_symbols)?;
```

## 设计模式

### 构建器模式

`ProjectBuilder` 使用构建器模式进行项目配置：

```rust
let mut builder = ProjectBuilder::new(config);
builder.set_custom_entry("src/main.cf")
       .set_target_triple("x86_64-unknown-linux-gnu")
       .set_force_static(true);
let executable = builder.compile()?;
```

### 策略模式

流水线使用策略模式进行编译阶段：

```rust
trait PipelineStage {
    fn execute(&mut self, context: &mut CompilationContext) -> Result<(), String>;
}
```

### 缓存模式

模块缓存使用缓存模式避免重新解析：

```rust
if let Some(cached) = self.module_cache.read().unwrap().get(path) {
    return Ok(cached.clone());
}
```

### 观察者模式

诊断发射器使用观察者模式进行错误报告：

```rust
self.emitter.emit(diagnostic);
```

## 性能考虑

### 模块缓存

- 解析的模块被缓存以避免重新解析
- 缓存在编译单元之间共享
- 减少大型项目的编译时间

### 并行编译

- 依赖图支持并行编译
- 独立模块可以同时编译
- 使用 rayon 进行并行处理

### 增量编译

- 仅重新编译更改的模块
- 依赖跟踪确保正确重建
- 基于时间戳的失效

### 内存效率

- Arc/RwLock 用于共享状态
- 模块的延迟加载
- 高效的数据结构

## 错误处理最佳实践

### 错误恢复

- 在错误后继续解析
- 收集多个错误
- 提供有用的消息

### 错误上下文

- 包含源位置
- 显示代码片段
- 提供建议

### 错误分类

- 按类型分类错误
- 使用错误代码进行参考
- 提供严重性级别

## 未来增强

- 增量编译支持
- 更好的并行编译
- 模块版本控制
- 热重载支持
- 分布式编译
- 编译缓存持久化
- 更好的错误恢复
- 更详细的统计
- 性能分析支持
- 编译时间预测

## 另请参阅

- [解析器模块](parser.md) - 源代码解析
- [语义分析模块](semantic.md) - 语义分析
- [类型系统模块](types.md) - 类型检查
- [后端模块](backend.md) - 代码生成
- [C 集成模块](c_integration.md) - C 语言 FFI