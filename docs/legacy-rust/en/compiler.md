# Compiler Frontend Module

## Overview

The Compiler Frontend module (`src/compiler/`) orchestrates the entire Coffee compilation process, from source file discovery to final executable generation. It manages project configuration, dependency resolution, parallel compilation, and linking.

## Module Structure

```
src/compiler.rs              # Frontend module: CompilationResult, CompilerFrontend
src/compiler/
├── pipeline/                # CompilationPipeline (parse → imports → sema → types → MIR)
│   ├── mod.rs
│   ├── parse.rs
│   ├── imports.rs
│   └── collect.rs
├── pkg/                     # fetch, cache, tree hash, import-root resolve
├── project.rs               # Project configuration (coffee.toml), PackageDep
├── session.rs               # Compilation session management
├── scanner.rs               # Source file discovery
├── builder.rs               # Project build coordinator
├── entry_point.rs           # Entry point management
├── scheduler.rs             # Compilation scheduling
├── graph.rs                 # Dependency graph
├── import_resolver.rs       # Import resolution
├── linker.rs                # Object file linking
└── unit.rs                  # Compilation units
```

## Core Components

### 1. Compilation Pipeline (`pipeline/`)

Coordinates all compilation phases. Live in `src/compiler/pipeline/` (`mod.rs`, `parse.rs`, `imports.rs`, `collect.rs`). There is no `pipeline.rs`.

```rust
pub struct CompilationPipeline {
    session: Arc<Session>,
}

impl CompilationPipeline {
    pub fn new() -> Self { /* default Session */ }
    pub fn with_session(session: Arc<Session>) -> Self { /* ... */ }
    pub fn compile(&self, source: &str, file_name: Option<&str>) -> CompilationResult {
        // 1. Parse source code
        // 2. Process imports
        // 3. Semantic analysis (declare, then bodies)
        // 4. Type checking
        // 5. Lower functions to MirFn (hir_fns)
        // 6. CompilationResult
    }
}
```

**Compilation Phases:**
1. **Parsing**: Source code → AST (`src/parser/`, expressions in `expr/`)
2. **Import Processing**: Resolve and load imported modules
3. **Semantic Analysis**: `src/semantic/analyzer/`
4. **Type Checking**: `src/types/checker/`
5. **MIR lowering**: statement-list CFG, not SSA. Complete MIR covers `if`/`while`, range and collection `for` (as `ForRange`), `match` (as `If`), and `raise`. Nested `class`/`fn` are `MirStmt::Nested(NestedDecl)` (not a cloned `Statement`). Complete MIR never contains leftover `Match`/`ForIn`; unlowerable `match`/`for` omit the function from `hir_fns` (`lower_function` Err is an Error diagnostic). Typechecked `p.x` then `rm` still produces complete MIR.
6. **Codegen (driver)**: `compile_program_with_hir`. A `MirFn` that reaches codegen is `complete: true` and has no leftover `Match`/`ForIn`. Complete MIR compiles in `mir_gen.rs`. Omitted `hir_fns` is a compile error (`missing MIR for function`), not an AST body. `compile_program` is a dead trap (do not compile from empty MIR). `compile_statement` Match/For/Raise already error if hit.

### 2. Project Configuration (`project.rs`)

Manages `coffee.toml` configuration files.

**Configuration Structure:**
```toml
[package]
name = "project_name"
version = "0.1.0"
authors = ["Author <email@example.com>"]
description = "Project description"

[dependencies]
# std is bundled (`coffee std install`). Do not set a machine-local path= to std
# unless you intend to override with [dependencies.packages.std].

# path XOR (url + SHA-256 of unpacked tree). Not semver foo = "1.0".
# [dependencies.packages.foo]
# url = "https://example.com/foo.tar.gz"
# hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
# [dependencies.packages.local_bar]
# path = "../bar"

[dependencies.c_libraries.libm]
name = "m"
headers = ["math.h"]
static_link = false

[build]
main = "src/main"
src_dir = "src"
target_dir = "target"
have_c = true

[target]
opt_level = 2
triple = "x86_64-unknown-linux-gnu"
```

**Configuration Types:**
```rust
pub struct ProjectConfig {
    pub package: Package,
    pub dependencies: Dependencies,
    pub build: BuildConfig,
    pub target: TargetConfig,
}

/// One `[dependencies.packages.<key>]`: `path` XOR (`url` and `hash`).
pub struct PackageDep {
    pub path: Option<String>,
    pub url: Option<String>,
    pub hash: Option<String>,
}
```

**Packages (`src/compiler/pkg/`):** Zig-style, no registry. `coffee fetch` downloads url+hash deps into `$COFFEE_CACHE` (else `$XDG_CACHE_HOME/coffee`, else `~/.cache/coffee`). `coffee fetch <url>` prints the tree hash; `--save [name]` writes the toml key (name defaults to the fetched `[package].name`). Project compile (`ProjectBuilder`) resolves the graph and prepends import roots. Path deps are local directories; url deps are `.tar.gz`.

**Official std:** embedded from `library/std`. `coffee std install` extracts it. Compile prepends that import root unless the project already has package key `std`. User code `use print in std`; `of c` lives in `sys.cf`.

### 3. Compilation Session (`session.rs`)

Manages shared state across compilation.

```rust
pub struct Session {
    pub analyzer: Arc<RwLock<SemanticAnalyzer>>,
    pub type_checker: Arc<RwLock<TypeChecker>>,
    pub emitter: DiagnosticEmitter,
    pub config: CompilerConfig,
    pub c_imports: Arc<RwLock<Vec<String>>>,
    pub cfc_symbols: Arc<RwLock<HashMap<String, CSymbolTable>>>,
}
```

**Session Features:**
- Thread-safe shared state
- Diagnostic collection
- C import tracking
- Configuration management

### 4. Source Scanner (`scanner.rs`)

Discovers Coffee source files.

```rust
pub struct SourceScanner {
    src_dir: PathBuf,
}

impl SourceScanner {
    pub fn scan(&self) -> Result<Vec<PathBuf>, String> {
        // Recursively scan for .cf files
    }
}
```

**Scanning Features:**
- Recursive directory traversal
- .cf file extension filtering
- Error handling for inaccessible directories

### 5. Project Builder (`builder.rs`)

Coordinates the entire build process.

```rust
pub struct ProjectBuilder {
    config: ProjectConfig,
    entry_manager: EntryPointManager,
    units: CompilationUnits,
    frontend: CompilerFrontend,
    target_triple: Option<String>,
    force_static: bool,
}

impl ProjectBuilder {
    pub fn compile(&mut self) -> Result<PathBuf, String> {
        // 1. Scan sources
        // 2. Build compilation units
        // 3. Validate project
        // 4. Schedule compilation
        // 5. Compile modules in parallel
        // 6. Link object files
        // 7. Generate C headers (if enabled)
    }
}
```

**Build Process:**
1. **Source Discovery**: Scan source directory for .cf files
2. **Unit Creation**: Create compilation units for each file
3. **Validation**: Check entry points and circular dependencies
4. **Dependency Analysis**: Build dependency graph
5. **Scheduling**: Create compilation batches
6. **Parallel Compilation**: Compile modules concurrently
7. **Linking**: Link object files into executable
8. **Header Generation**: Generate C headers if enabled

### 6. Compilation Scheduler (`scheduler.rs`)

Manages parallel compilation with dependency tracking.

```rust
pub struct CompilationScheduler {
    units: HashMap<String, CompilationUnit>,
    graph: DependencyGraph,
}

impl CompilationScheduler {
    pub fn schedule(&mut self) -> Result<Vec<Vec<String>>, String> {
        // Create compilation batches based on dependencies
    }
}
```

**Scheduling Features:**
- Topological sorting of dependencies
- Parallel compilation of independent modules
- Progress reporting
- Batch compilation

### 7. Dependency Graph (`graph.rs`)

Tracks module dependencies.

```rust
pub struct DependencyGraph {
    nodes: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn has_cycles(&self) -> bool {
        // Detect circular dependencies
    }
}
```

**Graph Features:**
- Dependency tracking
- Cycle detection
- Topological sorting

### 8. Import Resolver (`import_resolver.rs`)

Handles module imports.

```rust
pub struct ImportResolver {
    config: CompilerConfig,
    session: Arc<Session>,
    loaded_modules: HashMap<String, Program>,
}

impl ImportResolver {
    pub fn process_import(&mut self, import: &Import) -> Result<(), Diagnostic> {
        // Resolve and load imported modules
    }
}
```

**Import Resolution:**
- Search paths
- Module caching
- Circular import detection
- C library import handling

### 9. Linker (`linker.rs`)

Links object files into executables.

```rust
pub struct Linker {
    output_path: PathBuf,
    options: LinkOptions,
}

impl Linker {
    pub fn link(&self, object_files: &[PathBuf]) -> Result<(), String> {
        // Link object files with system linker
    }
}
```

**Linking Features:**
- Static/dynamic linking
- C library integration
- Library search paths
- Linker flags

### 10. Compilation Unit (`unit.rs`)

Represents a single module to compile.

```rust
pub struct CompilationUnit {
    pub name: String,
    pub source: PathBuf,
    pub object: PathBuf,
    pub dependencies: Vec<String>,
    pub hash: String,
    pub status: CompilationStatus,
}

pub enum CompilationStatus {
    Pending,
    Compiled(PathBuf),
    Failed(String),
}
```

**Unit Features:**
- Dependency tracking
- Hash-based caching
- Status management
- Incremental compilation

### 11. Entry Point Manager (`entry_point.rs`)

Manages program entry points.

```rust
pub struct EntryPointManager {
    entry_file: PathBuf,
}

impl EntryPointManager {
    pub fn validate<F1, F2>(&self, exists: F1, count_main: F2) -> Result<(), String>
    where
        F1: Fn(&Path) -> bool,
        F2: Fn(&Path) -> usize,
    {
        // Validate entry point exists and has exactly one main()
    }
}
```

### 12. Compiler Statistics

`CompilationStatistics` lives on `CompilationResult` in `src/compiler.rs` (there is no `statistics.rs`).

## Compilation Flow

### Single File Compilation

```
Source File (.cf)
    ↓
Parser
    ↓
AST
    ↓
Semantic Analyzer
    ↓
Type Checker
    ↓
Backend (LLVM)
    ↓
Object File (.o)
    ↓
Linker
    ↓
Executable
```

### Project Compilation

```
Project Directory
    ↓
Source Scanner
    ↓
Source Files (.cf)
    ↓
Build Units
    ↓
Dependency Graph
    ↓
Compilation Scheduler
    ↓
Parallel Compilation (Batches)
    ↓
Object Files (.o)
    ↓
Linker
    ↓
Executable (+ C Headers)
```

## Incremental Compilation

The compiler supports incremental compilation through:
1. **Hash-based caching**: Each source file has a hash
2. **Dirty detection**: Compare hashes to detect changes
3. **Selective compilation**: Only compile changed modules
4. **Dependency tracking**: Recompile dependents of changed modules

**Cache Logic:**
```rust
if unit.check_hash_cache()? {
    // Use cached object file
    unit.status = CompilationStatus::Compiled(unit.object.clone());
} else {
    // Recompile module
    compile_module(&mut unit)?;
}
```

## Error Handling

**Compilation Errors:**
```rust
pub struct CompilationResult {
    pub success: bool,
    pub program: Program,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    pub hints: Vec<Diagnostic>,
    pub report: AnalysisReport,
    pub stats: CompilationStatistics,
    pub c_imports: Vec<String>,
    pub cfc_symbols: HashMap<String, CSymbolTable>,
    /// Complete MIR compiles in mir_gen.rs; omitted hir_fns is a compile error.
    pub hir_fns: Vec<hir::MirFn>,
}
```

## Usage Example

### Single File Compilation

```rust
use coffee::compiler::{CompilationPipeline, Session};

let session = Arc::new(Session::new());
let pipeline = CompilationPipeline::new();

let source = r#"
fn main() => ():
    printf("Hello, World!\n")
"#;

let result = pipeline.compile(source, Some("main.cf"));

if result.success {
    println!("Compilation successful!");
} else {
    for error in result.errors {
        eprintln!("Error: {}", error.format());
    }
}
```

### Project Compilation

```rust
use coffee::compiler::{ProjectBuilder, ProjectConfig};

// Load configuration
let config = ProjectConfig::from_file(Path::new("coffee.toml"))?;

// Create builder
let mut builder = ProjectBuilder::new(config);

// Compile project
match builder.compile() {
    Ok(output_path) => {
        println!("Compilation successful: {}", output_path.display());
    }
    Err(e) => {
        eprintln!("Compilation failed: {}", e);
    }
}
```

## Design Considerations

### 1. Parallel Compilation
- Independent modules compiled concurrently
- Dependency-aware scheduling
- Efficient resource utilization

### 2. Incremental Builds
- Hash-based change detection
- Selective recompilation
- Fast rebuild times

### 3. Error Recovery
- Continue compilation after errors
- Comprehensive error reporting
- Helpful error messages

### 4. Cross-Compilation
- Target triple support
- Platform-specific handling
- ABI compatibility

## Future Enhancements

- Better parallelization strategies
- Distributed compilation
- Persistent compilation cache
- Build system integration (Make, Ninja)
- IDE integration
- Hot reloading