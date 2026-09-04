mod parser;
mod types;
mod semantic;
mod compiler;
mod diagnostics;
mod backend;
mod c;
mod library_finder;
#[macro_use]
mod debug_log;

use diagnostics::{Diagnostic, ErrorKind, Severity};


use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use fs2::FileExt;

/// 进程互斥锁 - 使用跨平台文件锁确保同一时间只有一个编译器实例运行
/// 当进程退出时（包括正常退出和 process::exit），操作系统会自动释放文件锁
/// 使用 fs2 crate 提供真正的跨平台文件锁（Unix: flock, Windows: LockFileEx）
/// 
/// 锁文件基于项目目录为单位，而不是工作目录：
/// - 如果存在coffee.toml，使用项目目录（coffee.toml所在目录）
/// - 如果不存在coffee.toml，使用工作目录
/// - 测试模式下跳过文件锁（通过环境变量COFFEE_TEST_MODE=1启用）
struct CompilerLock {
    _lock_file: Option<fs::File>,
    lock_path: Option<PathBuf>,
}

impl CompilerLock {
    /// 尝试获取编译器锁
    /// 如果已有其他编译器实例在运行，返回错误
    /// 
    /// 锁文件基于项目目录生成：
    /// - 如果存在coffee.toml，使用项目目录（coffee.toml所在目录）
    /// - 如果不存在coffee.toml，使用工作目录
    /// 
    /// 测试模式下跳过文件锁（通过环境变量COFFEE_TEST_MODE=1启用）
    fn acquire() -> Result<Self, String> {
        use std::io::Write;

        // 检查是否为测试模式
        if env::var("COFFEE_TEST_MODE").is_ok() {
            // 测试模式下不获取锁
            return Ok(CompilerLock {
                _lock_file: None,
                lock_path: None,
            });
        }

        // 确定锁文件位置 - 基于项目目录
        let lock_path = Self::get_lock_path()?;

        // 打开锁文件（如果不存在则创建）
        let mut lock_file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&lock_path)
            .map_err(|e| format!("error: failed to open lock file: {}", e))?;

        // 尝试获取排他锁（非阻塞模式）
        // fs2::FileExt::try_lock_exclusive() 在所有平台上提供真正的文件锁：
        // - Unix: 使用 flock(LOCK_EX | LOCK_NB)
        // - Windows: 使用 LockFileEx
        lock_file.try_lock_exclusive()
            .map_err(|e| {
                format!(
                    "error: another compiler instance is already running for this project\n  = note: if this is incorrect, manually delete the lock file: {:?}\n  = details: {}",
                    lock_path, e
                )
            })?;

        // 写入当前进程 ID
        let pid = std::process::id();
        let _ = writeln!(&lock_file, "{}", pid);

        // 刷新到磁盘
        let _ = lock_file.flush();

        Ok(CompilerLock {
            _lock_file: Some(lock_file),
            lock_path: Some(lock_path),
        })
    }

    /// 获取锁文件路径
    /// 基于项目目录或工作目录生成锁文件路径
    fn get_lock_path() -> Result<PathBuf, String> {
        // 尝试查找项目配置文件
        let project_dir = Self::find_project_directory();

        // 使用项目目录或工作目录
        let base_dir = project_dir.unwrap_or_else(|| {
            env::current_dir()
                .expect("failed to get current directory")
        });

        // 生成锁文件路径
        let lock_path = base_dir.join(".coffee_compiler.lock");
        Ok(lock_path)
    }

    /// 查找项目目录（coffee.toml所在目录）
    /// 从当前目录向上搜索，直到找到coffee.toml或到达根目录
    fn find_project_directory() -> Option<PathBuf> {
        let mut current_dir = env::current_dir().ok()?;

        loop {
            // 检查当前目录是否有coffee.toml
            let config_path = current_dir.join("coffee.toml");
            if config_path.exists() {
                return Some(current_dir);
            }

            // 尝试向上移动到父目录
            match current_dir.parent() {
                Some(parent) if parent != current_dir => {
                    current_dir = parent.to_path_buf();
                }
                _ => {
                    // 已到达根目录，没有找到coffee.toml
                    return None;
                }
            }
        }
    }
}

/// 当 Lock 被丢弃时释放锁
/// 注意：操作系统会在进程退出时自动释放文件锁，所以这里是清理文件
impl Drop for CompilerLock {
    fn drop(&mut self) {
        // 测试模式下没有锁文件，无需清理
        if self._lock_file.is_none() {
            return;
        }

        // 释放文件锁（fs2 会在关闭文件时自动释放）
        // 然后删除锁文件
        if let Some(lock_path) = &self.lock_path {
            let _ = fs::remove_file(lock_path);
        }
    }
}

/// Security: Validate file path to prevent directory traversal attacks
/// Returns an error if the path contains suspicious patterns
fn validate_file_path(file_path: &str) -> Result<PathBuf, String> {
    // Check for null bytes (prevents string injection)
    if file_path.contains('\0') {
        return Err(format!(
            "security error: null byte detected in path\n  = note: null bytes can be used to bypass path validation"
        ));
    }

    // Check for obvious path traversal attempts
    if file_path.contains("..") {
        return Err(format!(
            "security error: path traversal detected in '{}'\n  = note: parent directory references (..) are not allowed for security reasons",
            file_path
        ));
    }

    // Normalize the path
    let path = PathBuf::from(file_path);

    // Check if path is a symbolic link BEFORE resolving
    // This prevents symlink attacks
    if path.is_symlink() {
        return Err(format!(
            "security error: symbolic links are not allowed\n  = note: path '{}' is a symbolic link, which could bypass security checks",
            file_path
        ));
    }

    // Convert to absolute path for validation
    let absolute_path = if path.is_absolute() {
        path.clone()
    } else {
        match env::current_dir() {
            Ok(cwd) => cwd.join(&path),
            Err(_e) => {
                return Err(format!("failed to get current directory"));
            }
        }
    };

    // Canonicalize to resolve any . components
    // Note: We already checked for symlinks, but canonicalize may still traverse them
    // We need to be extra careful here
    match absolute_path.canonicalize() {
        Ok(canonical) => {
            // CRITICAL-8 FIX: Verify the canonical path is still within allowed directories
            // This prevents symlink attacks like: ln -s /etc/passwd ./safe_file.coffee
            let cwd = env::current_dir()
                .map_err(|e| format!("failed to get current directory: {}", e))?;

            // Check if canonical path starts with current directory
            if !canonical.starts_with(&cwd) {
                return Err(format!(
                    "security error: path '{}' is outside current working directory\n  = note: symbolic links or parent directory references are not allowed",
                    file_path
                ));
            }

            // Additional check: ensure the canonical path doesn't access sensitive system directories
            let path_str = canonical.to_string_lossy();

            // Block access to sensitive system directories (but allow /root itself for development)
            let blocked_prefixes = [
                "/etc/", "/sys/", "/proc/", "/dev/",
                "/usr/bin/", "/usr/sbin/", "/bin/", "/sbin/"
            ];

            for prefix in &blocked_prefixes {
                if path_str.starts_with(prefix) {
                    return Err(format!(
                        "security error: access to system directory '{}' is blocked\n  = note: attempting to access {}",
                        file_path, prefix.trim_end_matches('/')
                    ));
                }
            }

            Ok(canonical)
        }
        Err(_e) => {
            // File doesn't exist or isn't accessible - this is okay, will be caught later
            // But still verify the normalized path is within allowed directory
            let cwd = env::current_dir()
                .map_err(|e| format!("failed to get current directory: {}", e))?;

            if !absolute_path.starts_with(&cwd) {
                return Err(format!(
                    "security error: path '{}' is outside current working directory",
                    file_path
                ));
            }

            Ok(absolute_path)
        }
    }
}

/// 检查当前目录是否有 coffee.toml 项目配置文件
fn detect_project_config() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    let config_path = cwd.join("coffee.toml");

    if config_path.exists() {
        Some(config_path)
    } else {
        None
    }
}

/// 项目模式编译（检测到 coffee.toml）
fn compile_project(
    config_path: &Path,
    entry_file: Option<&str>,
    output_file: Option<&str>,
    _opt_level: u8,
    show_stats: bool,
    target_triple: Option<&String>,
    force_static: bool,
    _static_libs: &[String],
    emit: compiler::EmitKind,
) -> Result<(), String> {
    // Load project configuration
    let config = compiler::ProjectConfig::from_file(config_path)?;

    // Create project builder
    let mut builder = compiler::ProjectBuilder::new(config);

    // If entry file is specified, use it as the entry point instead of config
    if let Some(entry) = entry_file {
        builder.set_custom_entry(entry);
    }

    // Set target triple if specified
    if let Some(target) = target_triple {
        builder.set_target_triple(target);
    }
    
    // Set force_static if specified
    if force_static {
        builder.set_force_static(true);
        println!("Forcing static linking for all libraries");
    }

    builder.set_emit(emit);
    builder.set_output_file(output_file.map(PathBuf::from));

    // Compile the project
    let artifact = builder.compile()?;

    // Show statistics if requested
    if show_stats {
        println!("\nBuild Statistics:");
        println!("   Output: {}", artifact.display());
    }

    Ok(())
}

/// Note: This function is not currently used.
/// Single-file compilation is handled directly in main() to support the existing workflow.
/// Project mode uses compile_project() instead.
#[allow(dead_code)]
fn compile_single_file(_input_file: &str, _output_file: Option<&str>, _opt_level: u8, _show_stats: bool) -> Result<(), String> {
    // Single file compilation is integrated directly into main() function
    // This placeholder exists for API compatibility but is not called
    Ok(())
}

/// Search for a library file in common locations
/// Searches in: current directory, ./lib, $PATH directories, /usr/lib, /usr/local/lib, ~/.local/lib
fn find_library_file(lib_name: &str) -> Option<String> {
    use std::path::Path;

    // Possible library file names
    let lib_filenames = if lib_name.starts_with("lib") {
        vec![
            format!("{}.so", lib_name),
            format!("{}.a", lib_name),
        ]
    } else {
        vec![
            format!("lib{}.so", lib_name),
            format!("lib{}.a", lib_name),
        ]
    };

    // Search paths in order of priority
    let mut search_paths = Vec::new();

    // 1. Current directory
    search_paths.push(".".to_string());

    // 2. ./lib subdirectory
    search_paths.push("./lib".to_string());

    // 3. $PATH directories (for user-installed libraries)
    if let Ok(path_env) = std::env::var("PATH") {
        for path_dir in path_env.split(':') {
            if !path_dir.is_empty() {
                search_paths.push(path_dir.to_string());
            }
        }
    }

    // 4. System library paths
    search_paths.push("/usr/lib".to_string());
    search_paths.push("/usr/local/lib".to_string());
    search_paths.push("/lib".to_string());
    search_paths.push("/lib64".to_string());

    // 5. User local library path
    if let Ok(home) = std::env::var("HOME") {
        search_paths.push(format!("{}/.local/lib", home));
    }

    // Search for the library file
    for search_path in &search_paths {
        for lib_filename in &lib_filenames {
            let lib_path = Path::new(search_path).join(lib_filename);
            if lib_path.exists() {
                return Some(lib_path.to_string_lossy().to_string());
            }
        }
    }

    None
}

fn main() {
    // 解析命令行参数以检测测试模式
    let args: Vec<String> = env::args().collect();
    
    // 检查是否启用了测试模式
    let test_mode = args.iter().any(|arg| arg == "--test-mode");
    
    // 如果启用了测试模式，设置环境变量
    if test_mode {
        unsafe {
            env::set_var("COFFEE_TEST_MODE", "1");
        }
    }

    // 获取进程互斥锁 - 确保同一时间只有一个编译器实例运行
    // 文件锁基于项目目录生成，测试模式下跳过锁
    let _lock = match CompilerLock::acquire() {
        Ok(lock) => lock,
        Err(_e) => {
            process::exit(1);
        }
    };

    // 运行实际的主逻辑并设置退出码
    let exit_code = run_actual_main();

    // 正常退出 - Drop trait 会被调用，锁文件会被删除
    std::process::exit(exit_code);
}

/// 实际的主逻辑
/// 返回退出码（0 = 成功，1 = 失败）
fn run_actual_main() -> i32 {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        // No input file provided - check for project config
        let project_config = detect_project_config();
        if let Some(config_path) = project_config {
            // Project mode: coffee.toml exists, no input file needed
            if let Err(e) = compile_project(&config_path, None, None, 0, false, None, false, &[], compiler::EmitKind::Binary) {
                eprintln!("{}", e);
                return 1;
            }
            return 0;
        }
        print_usage();
        return 1;
    }

    // Parse command line arguments
    let mut input_file = None;
    let mut user_program_args = Vec::new();  // Collect user program arguments for JIT mode
    let mut output_file = None;
    let mut emit_ast = false;
    let mut emit_llvm = false;
    let mut emit_bc = false;
    let mut emit_asm = false;
    let mut show_stats = false;
    let mut show_memory = false;  // Show memory layout report
    let mut enable_bitfields = false;  // Enable bit fields support
    let mut enable_safety = false;  // Enable runtime safety checks
    let mut opt_level = 0u8;
    let mut link_mode = false;
    let mut force_static = false;  // Force static linking for all libraries
    let mut static_libs: Vec<String> = Vec::new(); // Specific libraries to link statically
    let mut bin_mode = false;
    let mut jit_mode = false;
    let mut gen_cfc = false;  // Generate .cfc file from .h/.c
    let mut include_paths: Vec<String> = Vec::new();  // Additional include paths for -c
    let mut target_triple: Option<String> = None;  // Target triple for cross-compilation
    let mut have_c = false;  // Generate C header files for FFI

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_usage();
                return 0;
            }
            "--version" | "-v" => {
                println!("Coffee Compiler v0.1.0");
                return 0;
            }
            "--emit-ast" => emit_ast = true,
            "--emit-llvm" => emit_llvm = true,
            "--emit-bc" => emit_bc = true,
            "--emit-asm" | "-S" => emit_asm = true,
            "--stats" => show_stats = true,
            "--show-memory" => show_memory = true,
            "--enable-bitfields" => enable_bitfields = true,
            "--enable-safety" => enable_safety = true,
            "--link" | "-l" => link_mode = true,
            "--bin" | "-b" => bin_mode = true,
            "--gen-cfc" | "-c" => gen_cfc = true,
            "--have-c" => have_c = true,
            "--jit" | "-j" => jit_mode = true,
            "--test-mode" => {
                // 测试模式：已在main函数中处理，这里跳过
            }
            "-O0" => opt_level = 0,
            "-O1" => opt_level = 1,
            "-O2" => opt_level = 2,
            "-O3" => opt_level = 3,
            "-I" => {
                // Collect include paths for .cfc generation
                i += 1;
                if i < args.len() {
                    include_paths.push(args[i].clone());
                }
            }
            "--static" => {
                // Force static linking for all libraries
                force_static = true;
            }
            "--static-lib" => {
                // Link specific library statically
                i += 1;
                if i < args.len() {
                    static_libs.push(args[i].clone());
                }
            }
            "--target" => {
                // Target triple for cross-compilation
                i += 1;
                if i < args.len() {
                    target_triple = Some(args[i].clone());
                }
            }
            "-o" => {
                i += 1;
                if i < args.len() {
                    output_file = Some(args[i].clone());
                }
            }
            "init" => {
                // Initialize a new Coffee project
                if let Err(e) = init_project() {
                    eprintln!("{}", e);
                    return 1;
                }
                return 0;
            }
            _ if args[i].starts_with('-') => {
                return 1;
            }
            _ => {
                // First .cf file is the input file
                // Subsequent arguments are user program arguments (for JIT mode)
                if input_file.is_none() && args[i].ends_with(".cf") {
                    input_file = Some(args[i].clone());
                } else {
                    // Collect as user program argument
                    user_program_args.push(args[i].clone());
                }
            }
        }
        i += 1;
    }

    let input_file = match input_file {
        Some(f) => f,
        None => {
            return 1;
        }
    };

    // Check if input is .o file for linking mode
    let is_object_file = input_file.ends_with(".o");

    if link_mode {
        if !is_object_file {
            return 1;
        }
        // Link mode: directly link object files
        return link_object_files(&[&input_file], &output_file);
    }

    // Generate .cfc file from C header/source
    if gen_cfc {
        // Check if input is .h or .c file
        if !input_file.ends_with(".h") && !input_file.ends_with(".c") {
            let error_kind = ErrorKind::CodeGeneration {
                stage: "input validation".to_string(),
                details: format!("{}: -c flag requires .h or .c file", input_file),
            };
            let diag = Diagnostic::new(
                Severity::Error,
                error_kind.clone(),
                format!("error: {} requires .h or .c file", input_file)
            );
            eprintln!("{}", diag.format());
            return 1;
        }

        use c::generator;
        match generator::generate_cfc(&input_file, output_file.as_ref(), &include_paths) {
            Ok(output_path) => {
                println!("Generated .cfc file: {}", output_path);
                return 0;
            }
            Err(e) => {
                let error_kind = ErrorKind::CodeGeneration {
                    stage: "CFC generation".to_string(),
                    details: e,
                };
                let diag = Diagnostic::new(
                    Severity::Error,
                    error_kind.clone(),
                    error_kind.description()
                );
                eprintln!("{}", diag.format());
                return 1;
            }
        }
    }

    // Check for project configuration FIRST
    // If coffee.toml exists, use project mode
    // If no coffee.toml, use single-file mode
    let project_config = detect_project_config();

    if let Some(config_path) = project_config {
        // Project mode: coffee.toml exists
        println!("Coffee project detected");
        println!("   Config: {}", config_path.display());

        // Use input file as entry point if specified
        let entry_file = if input_file.ends_with(".cf") {
            Some(input_file.as_str())
        } else {
            None
        };

        let emit = if emit_llvm {
            compiler::EmitKind::LlvmIr
        } else if emit_bc {
            compiler::EmitKind::Bitcode
        } else if emit_asm {
            compiler::EmitKind::Assembly
        } else {
            compiler::EmitKind::Binary
        };

        if let Err(e) = compile_project(&config_path, entry_file, output_file.as_deref(), opt_level, show_stats, target_triple.as_ref(), force_static, &static_libs, emit) {
            eprintln!("{}", e);
            return 1;
        }
        return 0;
    }

    // Single-file mode: no coffee.toml
    // Security: Validate input file path before reading
    let validated_path = match validate_file_path(&input_file) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };

    // Read source file
    let source_code = match fs::read_to_string(&validated_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("error: failed to read source file: {}", e);
            return 1;
        }
    };

    // Compile - parse once and compile
    let mut frontend = compiler::CompilerFrontend::new();
    let result = frontend.compile(&source_code, Some(&input_file));

    if !result.success {
        for error in &result.errors {
            eprintln!("{}", error.format());
        }
        return 1;
    }

    // Show warnings and hints if any
    if !result.warnings.is_empty() || !result.hints.is_empty() {
        if !result.warnings.is_empty() {
            for warning in &result.warnings {
                eprintln!("{}", warning.format());
            }
        }
        if !result.hints.is_empty() {
            for hint in &result.hints {
                eprintln!("{}", hint.format());
            }
        }
    }

    // Show statistics if requested
    if show_stats {
        println!("{}", frontend.format_result(&result));
    }

    // Show imported modules if any
    if !result.imported_modules.is_empty() && show_stats {
        println!("  Imported modules:");
        for module in &result.imported_modules {
            println!("    - {}", module);
        }
    }

    // Output AST if requested
    if emit_ast {
        println!("// AST for {}", input_file);
        println!("// {} statements", result.stats.statements_parsed);
        for stmt in &result.program.statements {
            println!("{:?}", stmt);
        }
        return 0;
    }

    // Note: emit_llvm is now handled in the main compilation path below

    // JIT mode: compile and execute immediately
    if jit_mode {
        match backend::compile_and_run(&result.program, &result.c_imports, result.cfc_symbols, &user_program_args) {
            Ok(exit_code) => {
                println!("Program exited with code: {}", exit_code);
                return exit_code;
            }
            Err(e) => {
                eprintln!("{}", e);
                return 1;
            }
        }
    }

    // Compile to native code
    let output_path = output_file.unwrap_or_else(|| {
        let stem = Path::new(&input_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("a");
        if emit_asm {
            format!("{}.s", stem)
        } else if emit_bc {
            format!("{}.bc", stem)
        } else if emit_llvm {
            format!("{}.ll", stem)
        } else {
            format!("{}.o", stem)
        }
    });

    let opt = match opt_level {
        0 => inkwell::OptimizationLevel::None,
        1 => inkwell::OptimizationLevel::Less,
        2 => inkwell::OptimizationLevel::Default,
        _ => inkwell::OptimizationLevel::Aggressive,
    };

    let context = inkwell::context::Context::create();
    let llvm_backend = backend::Backend::with_target(&context, &input_file, target_triple.clone());

    let mut codegen = backend::codegen::CodeGenerator::new(&llvm_backend);
    codegen.enable_bitfields(enable_bitfields);
    codegen.enable_safety(enable_safety);
    if let Err(e) = codegen.compile_program(&result.program, &result.c_imports, result.cfc_symbols) {
        let error_kind = ErrorKind::CodeGeneration {
            stage: "LLVM IR generation".to_string(),
            details: e,
        };
        let diag = Diagnostic::new(
            Severity::Error,
            error_kind.clone(),
            error_kind.description()
        );
        eprintln!("{}", diag.format());
        return 1;
    }

    if let Err(e) = llvm_backend.verify() {
        let error_kind = ErrorKind::Verification {
            phase: "LLVM verification".to_string(),
            details: e,
        };
        let diag = Diagnostic::new(
            Severity::Error,
            error_kind.clone(),
            error_kind.description()
        );
        eprintln!("{}", diag.format());
        return 1;
    }

    llvm_backend.optimize(opt);

    let output_path = Path::new(&output_path);

    let write_result = if emit_asm {
        llvm_backend.write_assembly_file(output_path)
    } else if emit_bc {
        if llvm_backend.write_bitcode(&output_path) {
            Ok(())
        } else {
            Err("Failed to write bitcode".to_string())
        }
    } else if emit_llvm {
        llvm_backend.write_ir(&output_path)
            .map_err(|e| format!("Failed to write LLVM IR: {}", e))
            .map(|_| ())
    } else {
        llvm_backend.write_object_file(output_path)
    };

    match write_result {
        Ok(_) => {
            // Show memory layout report if there are structs
            if codegen.has_structs() {
                println!("{}", codegen.get_memory_layout_report());
            }

            // Generate C header file if requested
            if have_c {
                // Extract functions and classes from program statements
                let functions: Vec<crate::parser::function::Function> = result.program.statements.iter()
                    .filter_map(|stmt| {
                        if let crate::parser::Statement::Function(f) = stmt {
                            Some(f.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                let classes: Vec<crate::parser::class::ClassDef> = result.program.statements.iter()
                    .filter_map(|stmt| {
                        if let crate::parser::Statement::Class(c) = stmt {
                            Some(c.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                let header_output_dir = Path::new(&input_file).parent().unwrap_or(Path::new("."));
                let header_name = Path::new(&input_file).file_stem().unwrap().to_str().unwrap();
                let header_path = header_output_dir.join(format!("{}.h", header_name));
                
                if let Err(e) = c::header_gen::generate_header(
                    &functions,
                    &classes,
                    &header_path,
                    header_name,
                ) {
                    eprintln!("Warning: Failed to generate C header file: {}", e);
                }
            }

            // Check if this module has a main() entry point
            let has_main = llvm_backend.module.get_function("main").is_some();
            if !has_main {
                // This is a library module, not an executable
                eprintln!("Warning: This module does not contain a main() entry point.");
                eprintln!("  = note: It cannot be executed directly as a program.");
                eprintln!("  = note: This module can be used as a library (imported by other modules).");
                eprintln!("  = help: To make it executable, add a main statement:");
                eprintln!("        main(entry_function())");
                eprintln!("  = help: With command-line arguments:");
                eprintln!("        main(entry_function(arg1, arg2))");
            }

            if !emit_asm && !emit_llvm && !emit_bc && bin_mode {
                // Bin mode: automatically link to create executable
                let exe_path = output_path.with_extension("");

                use std::process::Command;

                // Build linker command with C library dependencies
                let mut linker_cmd = Command::new("clang");
                linker_cmd.arg(&output_path);

                // Add current directory to library search path for custom libraries
                linker_cmd.arg("-L.");

                // Add rpath to make executables find libraries in current directory
                // This allows running the executable without setting LD_LIBRARY_PATH
                linker_cmd.arg("-Wl,-rpath,.");
                linker_cmd.arg("-Wl,-rpath,$ORIGIN");

                // Add C library flags if any C imports were used
                let mut linked_libs = std::collections::HashSet::new();
                for c_lib in &result.c_imports {
                    // Extract library name from "library:symbol" format
                    let lib_name = if c_lib.contains(':') {
                        // Format is "library:symbol"
                        c_lib.split(':').next().unwrap_or(c_lib)
                    } else {
                        c_lib.as_str()
                    };

                    // Avoid duplicate library flags
                    if !linked_libs.contains(lib_name) {
                        linked_libs.insert(lib_name.to_string());

                        // Convert library name to linker flag
                        // e.g., "libc" -> "-lc", "libm" -> "-lm"
                        let lib_flag = if lib_name.starts_with("lib") {
                            format!("-l{}", &lib_name[3..])
                        } else if lib_name == "c" {
                            "-lc".to_string()
                        } else if lib_name == "m" {
                            "-lm".to_string()
                        } else {
                            format!("-l{}", lib_name)
                        };

                        // Check if custom library file exists (skip for standard libraries)
                        let is_std_lib = lib_name == "c" || lib_name == "m" || lib_name == "libc" || lib_name == "libm";
                        if !is_std_lib {
                            // Try to find the library file in common locations
                            match find_library_file(lib_name) {
                                Some(lib_path) => {
                                    // Add library directory to search path
                                    if let Some(lib_dir) = std::path::Path::new(&lib_path).parent() {
                                        let lib_dir_str = lib_dir.to_string_lossy().to_string();
                                        if lib_dir_str != "." {
                                            linker_cmd.arg(format!("-L{}", lib_dir_str));
                                            linker_cmd.arg(format!("-Wl,-rpath,{}", lib_dir_str));
                                        }
                                    }
                                    eprintln!("  = note: found library '{}' at '{}'", lib_name, lib_path);
                                }
                                None => {
                                    let error_kind = ErrorKind::LinkError {
                                        details: format!("library '{}' not found\n  = note: searched in: current directory, ./lib, $PATH, /usr/lib, /usr/local/lib, ~/.local/lib\n  = help: compile the library first or install it to a standard location",
                                            lib_name),
                                    };
                                    let diag = Diagnostic::new(
                                        Severity::Error,
                                        error_kind.clone(),
                                        error_kind.description()
                                    );
                                    eprintln!("{}", diag.format());
                                    return 1;
                                }
                            }
                        }

                        linker_cmd.arg(&lib_flag);
                    }
                }

                linker_cmd.arg("-o").arg(&exe_path);

                let linker_status = linker_cmd.output();

                match linker_status {
                    Ok(output) => {
                        if output.status.success() {
                            println!("Compiled {} -> {}", input_file, exe_path.display());

                            // Show memory layout report if requested
                            if show_memory && codegen.has_structs() {
                                println!("\n=== Memory Layout Report ===\n");
                                println!("{}", codegen.get_memory_layout_report());
                            }
                        } else {
                            // Link failed - parse and display helpful error
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let stdout = String::from_utf8_lossy(&output.stdout);

                            // Parse common linker errors
                            let mut error_details = String::new();

                            // Check for undefined symbol errors
                            if stderr.contains("undefined reference") {
                                // Extract undefined symbols
                                for line in stderr.lines() {
                                    if line.contains("undefined reference") {
                                        error_details.push_str(&format!("{}\n", line));
                                    }
                                }
                                error_details.push_str(&format!("\n  = help: undefined symbols indicate that a function or variable was referenced but not found. Check:\n"));
                                error_details.push_str(&format!("    1. The function name is spelled correctly\n"));
                                error_details.push_str(&format!("    2. The function is properly imported with 'use <name> in <library> of c'\n"));
                                error_details.push_str(&format!("    3. The library file exists and is properly linked\n"));
                            }
                            // Check for library not found errors
                            else if stderr.contains("cannot find -l") || stderr.contains("cannot open") {
                                for line in stderr.lines() {
                                    if line.contains("cannot find") || line.contains("cannot open") {
                                        error_details.push_str(&format!("{}\n", line));
                                    }
                                }
                                error_details.push_str(&format!("\n  = help: library file not found. Ensure:\n"));
                                error_details.push_str(&format!("    1. The library file (.so or .a) exists in the current directory or specified -L path\n"));
                                error_details.push_str(&format!("    2. The library name in the import statement matches the file (e.g., 'use x in hello of c' requires libhello.so)\n"));
                            }
                            // Other linker errors
                            else {
                                error_details.push_str(&stderr);
                                error_details.push_str(&stdout);
                            }

                            let error_kind = ErrorKind::LinkError {
                                details: error_details,
                            };
                            let diag = Diagnostic::new(
                                Severity::Error,
                                error_kind.clone(),
                                error_kind.description()
                            );
                            eprintln!("{}", diag.format());
                            return 1;
                        }
                    }
                    Err(e) => {
                        let error_kind = ErrorKind::LinkError {
                            details: format!("Failed to execute linker: {}", e),
                        };
                        let diag = Diagnostic::new(
                            Severity::Error,
                            error_kind.clone(),
                            error_kind.description()
                        );
                        eprintln!("{}", diag.format());
                        return 1;
                    }
                }
            } else {
                println!("Compiled {} -> {}", input_file, output_path.display());

                // Show memory layout report if requested
                if show_memory && codegen.has_structs() {
                    println!("\n=== Memory Layout Report ===\n");
                    println!("{}", codegen.get_memory_layout_report());
                }
            }
        }
        Err(e) => {
            let error_kind = ErrorKind::CodeGeneration {
                stage: "object file write".to_string(),
                details: format!("Failed to write output file: {}", e),
            };
            let diag = Diagnostic::new(
                Severity::Error,
                error_kind.clone(),
                error_kind.description()
            );
            eprintln!("{}", diag.format());
            return 1;
        }
    }

    // 成功编译
    0
}

/// Link object files to create an executable
/// Returns exit code (0 = success, 1 = failure)
fn link_object_files(object_files: &[&str], output_file: &Option<String>) -> i32 {
    let output_path: PathBuf = if let Some(out) = output_file {
        PathBuf::from(out)
    } else {
        // Default: use first object file name without .o extension
        let first_obj = Path::new(&object_files[0]);
        first_obj.with_extension("")
    };

    use std::process::Command;

    let mut cmd = Command::new("clang");
    for obj in object_files {
        cmd.arg(obj);
    }
    cmd.arg("-o").arg(&output_path);

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                println!("Linked {} -> {}", object_files.join(" "), output_path.display());
                0
            } else {
                1
            }
        }
        Err(_e) => {
            1
        }
    }
}

/// Initialize a new Coffee project in the current directory
/// Creates standard directory structure and configuration files
fn init_project() -> Result<(), String> {
    let cwd = env::current_dir()
        .map_err(|e| format!("failed to get current directory: {}", e))?;

    // Check if coffee.toml already exists
    let config_path = cwd.join("coffee.toml");
    if config_path.exists() {
        return Err(format!(
            "error: coffee.toml already exists in this directory\n  = note: this directory appears to be already initialized as a Coffee project"
        ));
    }

    println!("Initializing Coffee project");
    println!("  Directory: {}", cwd.display());

    // Create directory structure
    let src_dir = cwd.join("src");
    let lib_dir = cwd.join("lib");
    let target_dir = cwd.join("target");

    println!("Creating directory structure...");

    if !src_dir.exists() {
        fs::create_dir(&src_dir)
            .map_err(|e| format!("failed to create src/ directory: {}", e))?;
        println!("  Created src/");
    }

    if !lib_dir.exists() {
        fs::create_dir(&lib_dir)
            .map_err(|e| format!("failed to create lib/ directory: {}", e))?;
        println!("  Created lib/");
    }

    if !target_dir.exists() {
        fs::create_dir(&target_dir)
            .map_err(|e| format!("failed to create target/ directory: {}", e))?;
        println!("  Created target/");
    }

    // Create coffee.toml with comprehensive template
    println!("Creating coffee.toml...");
    let project_name = cwd.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("my_project");

    let config_content = format!(
        r#"# Coffee 项目配置文件

[package]
name = "{}"
version = "0.1.0"
authors = ["Your Name <you@example.com>"]
description = "My awesome Coffee project"

[dependencies]
# 标准库依赖（可选）
# std = "0.1.0"

# C 库依赖（可选）
# [dependencies.c_libraries.libm]
# name = "m"
# headers = ["math.h"]
# static_link = false  # 动态链接（默认）或 true（静态链接）

[build]
main = "src/main"
target_dir = "target"
src_dir = "src"

[target]
opt_level = 2
# triple = "x86_64-unknown-linux-gnu"
"#,
        project_name
    );

    fs::write(&config_path, config_content)
        .map_err(|e| format!("failed to create coffee.toml: {}", e))?;
    println!("  Created coffee.toml");

    // Create src/main.cf
    println!("Creating src/main.cf...");
    let main_content = r#"/#/ Main entry point
use printf in libc of c

fn main() => int:
    printf("Hello from Coffee!")
    return 0

main(main())

"#;

    let main_path = src_dir.join("main.cf");
    fs::write(&main_path, main_content)
        .map_err(|e| format!("failed to create src/main.cf: {}", e))?;
    println!("  Created src/main.cf");

    // Create lib/.gitkeep
    let gitkeep_path = lib_dir.join(".gitkeep");
    fs::write(&gitkeep_path, "")
        .map_err(|e| format!("failed to create lib/.gitkeep: {}", e))?;

    println!();
    println!("Project initialized successfully");
    println!();
    println!("Directory structure:");
    println!("  coffee.toml       # Project configuration");
    println!("  src/               # Source files");
    println!("    main.cf          # Main entry point");
    println!("  lib/               # C library (.cfc) files");
    println!("  target/            # Build output");
    println!();
    println!("Next steps:");
    println!("  1. Edit src/main.cf to write your code");
    println!("  2. Place .cfc files in lib/ for C library imports");
    println!("  3. Run 'coffee' to build the project");
    println!("  4. Run './target/main' to execute");
    println!();

    Ok(())
}

fn print_usage() {
    println!("Coffee Compiler v0.1.0");
    println!();
    println!("Usage: coffee [options] <input.cf>");
    println!("   or: coffee init         Initialize a new Coffee project");
    println!();
    println!("Commands:");
    println!("  init                    Initialize a new Coffee project in current directory");
    println!();
    println!("Options:");
    println!("  --help, -h              Show this help message");
    println!("  --version, -v           Show version information");
    println!("  --emit-ast              Emit AST (abstract syntax tree)");
    println!("  --emit-llvm             Emit LLVM IR (human-readable)");
    println!("  --emit-bc               Emit LLVM bitcode");
    println!("  --emit-asm, -S          Emit assembly");
    println!("  --stats                 Show compilation statistics");
    println!("  --show-memory           Show memory layout report");
    println!("  --enable-bitfields      Enable bit fields support");
    println!("  --enable-safety         Enable runtime safety checks");
    println!("  --link, -l              Link mode (link .o files)");
    println!("  --bin, -b               Binary mode (produce executable)");
    println!("  --gen-cfc, -c           Generate .cfc from .h/.c");
    println!("  --jit, -j               JIT execution mode");
    println!("  --target <triple>       Target triple for cross-compilation");
    println!("                          (e.g., x86_64-unknown-linux-gnu, aarch64-linux-android)");
    println!("  --static                Force static linking for all C libraries");
    println!("  --static-lib <name>     Link specific library statically");
    println!("  -O0, -O1, -O2, -O3      Set optimization level");
    println!("  -I <path>               Add include path for .cfc");
    println!("  -o <file>               Specify output file");
}

