# Compiler Frontend Module

## Overview

The Compiler Frontend module (`src/compiler/`) orchestrates the entire Coffee compilation process, from source file discovery to final executable generation. It manages project configuration, dependency resolution, parallel compilation, and linking.

## Module Structure

```
src/compiler/
├── mod.rs          # Main compiler module
├── pipeline.rs     # Compilation pipeline orchestration
├── project.rs      # Project configuration (coffee.toml)
├── session.rs      # Compilation session management
├── scanner.rs      # Source file discovery
├── builder.rs      # Project build coordinator
├── entry_point.rs  # Entry point management
├── scheduler.rs    # Compilation scheduling
├── graph.rs        # Dependency graph
├── import_resolver.rs # Import resolution
├── linker.rs       # Object file linking
├── module_loader.rs # Module loading
├── unit.rs         # Compilation units
└── statistics.rs   # Compilation statistics
```

## Core Components

### 1. Compilation Pipeline (`pipeline.rs`)

Coordinates all compilation phases.

```rust
pub struct CompilationPipeline {
    import_resolver: ImportResolver,
    session: Arc<Session>,
}

impl CompilationPipeline {
    pub fn compile(&self, source: &str, file_name: Option<&str>) -> CompilationResult {
        // 1. Parse source code
        // 2. Process imports
        // 3. Perform semantic analysis
        // 4. Type checking
        // 5. Generate compilation result
    }
}
```

**Compilation Phases:**
1. **Parsing**: Source code → AST
2. **Import Processing**: Resolve and load imported modules
3. **Semantic Analysis**: Scope and symbol resolution
4. **Type Checking**: Type validation
5. **Result Generation**: Diagnostics and statistics

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
std = "0.2.0"

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
```

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

### 12. Compiler Statistics (`statistics.rs`)

Tracks compilation metrics.

```rust
pub struct CompilationStatistics {
    pub line_count: usize,
    pub statement_count: usize,
    pub function_count: usize,
    pub class_count: usize,
    pub compilation_time: Duration,
}
```

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
}
```

## Usage Example

### Single File Compilation

```rust
use coffee::compiler::{CompilationPipeline, Session};

let session = Arc::new(Session::new());
let pipeline = CompilationPipeline::new(session);

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