use coffee::compiler;
use coffee::diagnostics::{Diagnostic, ErrorKind, Severity};
use coffee::backend;
use coffee::c;
use coffee::library_finder;


use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};
use fs2::FileExt;

/// Walk from cwd toward the filesystem root and return the first `coffee.toml`.
fn find_coffee_toml() -> Option<PathBuf> {
    let mut current_dir = env::current_dir().ok()?;
    loop {
        let config_path = current_dir.join("coffee.toml");
        if config_path.exists() {
            return Some(config_path);
        }
        match current_dir.parent() {
            Some(parent) if parent != current_dir => {
                current_dir = parent.to_path_buf();
            }
            _ => return None,
        }
    }
}

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
            .map_err(|e| {
                format!(
                    "error: failed to open lock file `{}`: {e}\n  = hint: check that the directory is writable",
                    lock_path.display()
                )
            })?;

        // 尝试获取排他锁（非阻塞模式）
        // fs2::FileExt::try_lock_exclusive() 在所有平台上提供真正的文件锁：
        // - Unix: 使用 flock(LOCK_EX | LOCK_NB)
        // - Windows: 使用 LockFileEx
        lock_file.try_lock_exclusive().map_err(|e| {
            format!(
                "error: could not acquire compiler lock `{}`\n  = note: another coffee process is compiling this project\n  = hint: wait for that compile to finish; use --test-mode only in the test harness\n  = details: {e}",
                lock_path.display()
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

    /// Directory that contains `coffee.toml`, walking up from cwd.
    fn find_project_directory() -> Option<PathBuf> {
        find_coffee_toml().and_then(|p| p.parent().map(|d| d.to_path_buf()))
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

/// Same search as the process lock: nearest `coffee.toml` walking up from cwd.
fn detect_project_config() -> Option<PathBuf> {
    find_coffee_toml()
}

/// Add `-L` / rpath for non-stdlib C imports; error if a custom library file is missing.
fn apply_c_import_library_search(
    opts: &mut compiler::linker::LinkOptions,
    c_imports: &[String],
    force_static: bool,
) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for c_lib in c_imports {
        let lib_name = compiler::linker::c_import_library_name(c_lib);
        let short_name = compiler::linker::lib_short_name(lib_name);
        if short_name.is_empty() || !seen.insert(short_name.clone()) {
            continue;
        }
        if short_name == "c" || short_name == "m" {
            continue;
        }
        match library_finder::find_library_file(lib_name) {
            Some(lib_path) => {
                if let Some(lib_dir) = Path::new(&lib_path).parent() {
                    let lib_dir_str = lib_dir.to_string_lossy().to_string();
                    if lib_dir_str != "." {
                        opts.lib_paths.push(lib_dir_str.clone());
                        if !force_static {
                            opts.link_flags
                                .push(format!("-Wl,-rpath,{}", lib_dir_str));
                        }
                    }
                }
                eprintln!("  = note: found library '{}' at '{}'", lib_name, lib_path);
            }
            None => {
                return Err(format!(
                    "library '{}' not found\n  = note: searched in: {}\n  = help: on Termux also check `$PREFIX/lib`; declare `[dependencies.c_libraries.{}]` in coffee.toml for a project C library; libc and libm are bundled (no extra search). compile or install the library if it is not bundled",
                    lib_name,
                    library_finder::searched_locations_note(),
                    short_name,
                ));
            }
        }
    }
    Ok(())
}

/// 项目模式编译（检测到 coffee.toml）
fn compile_project(
    config_path: &Path,
    entry_file: Option<&str>,
    output_file: Option<&str>,
    opt_level: Option<u8>,
    show_stats: bool,
    target_triple: Option<&String>,
    force_static: bool,
    static_libs: &[String],
    have_c: bool,
    enable_bitfields: bool,
    enable_safety: bool,
    show_memory: bool,
    emit: compiler::EmitKind,
) -> Result<PathBuf, String> {
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

    builder.set_static_lib_names(static_libs);

    if let Some(level) = opt_level {
        builder.set_opt_level(level);
    }
    builder.set_have_c(have_c);
    builder.set_enable_bitfields(enable_bitfields);
    builder.set_enable_safety(enable_safety);
    builder.set_show_memory(show_memory);

    builder.set_emit(emit);
    builder.set_output_file(output_file.map(PathBuf::from));

    // Compile the project
    let artifact = builder.compile()?;

    // Show statistics if requested
    if show_stats {
        println!("\nBuild Statistics:");
        println!("   Output: {}", artifact.display());
    }

    Ok(artifact)
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
        Err(e) => {
            eprintln!("{e}");
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
            if let Err(e) = compile_project(&config_path, None, None, None, false, None, false, &[], false, false, false, false, compiler::EmitKind::Binary) {
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
    let mut opt_level_cli = false;
    let mut link_mode = false;
    let mut force_static = false;  // Force static linking for all libraries
    let mut static_libs: Vec<String> = Vec::new(); // Specific libraries to link statically
    let mut bin_mode = false;
    let mut jit_mode = false;
    let mut gen_cfc = false;  // Generate .cfc file from .h/.c
    let mut include_paths: Vec<String> = Vec::new();  // Additional include paths for -c
    let mut target_triple: Option<String> = None;  // Target triple for cross-compilation
    let mut have_c = false;  // Generate C header files for FFI
    let mut run_mode = false;
    let mut test_cmd = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_usage();
                return 0;
            }
            "--version" | "-v" => {
                println!("Coffee Compiler v{}", compiler::compiler_display_version());
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
            "--" => {
                user_program_args.extend(args[i + 1..].iter().cloned());
                break;
            }
            "-O0" => {
                opt_level = 0;
                opt_level_cli = true;
            }
            "-O1" => {
                opt_level = 1;
                opt_level_cli = true;
            }
            "-O2" => {
                opt_level = 2;
                opt_level_cli = true;
            }
            "-O3" => {
                opt_level = 3;
                opt_level_cli = true;
            }
            "-I" => {
                i += 1;
                if i >= args.len() {
                    return cli_arg_error("-I requires a path");
                }
                include_paths.push(args[i].clone());
            }
            "--static" => {
                // Force static linking for all libraries
                force_static = true;
            }
            "--static-lib" => {
                i += 1;
                if i < args.len() {
                    static_libs.push(args[i].clone());
                } else if input_file.is_none() {
                    // Bare `--static-lib` (no source) is an incomplete argv;
                    // `file.cf --static-lib` with no name is ignored on purpose.
                    return cli_arg_error("--static-lib requires a library name");
                }
            }
            "--target" => {
                i += 1;
                if i >= args.len() {
                    return cli_arg_error("--target requires a triple");
                }
                target_triple = Some(args[i].clone());
            }
            "-o" => {
                i += 1;
                if i >= args.len() {
                    return cli_arg_error("-o requires a file path");
                }
                output_file = Some(args[i].clone());
            }
            "init" => {
                // Initialize a new Coffee project
                if let Err(e) = init_project() {
                    eprintln!("{}", e);
                    return 1;
                }
                return 0;
            }
            "run" => {
                run_mode = true;
            }
            "test" => {
                test_cmd = true;
            }
            "fetch" => {
                let rest: Vec<String> = args[i + 1..].to_vec();
                if let Err(e) = cmd_fetch(&rest) {
                    eprintln!("{}", e);
                    return 1;
                }
                return 0;
            }
            "std" => {
                i += 1;
                if i >= args.len() || args[i] != "install" {
                    eprintln!("error: expected `coffee std install [dir]`");
                    eprintln!("Try 'coffee --help' for more information.");
                    return 1;
                }
                i += 1;
                let mut dest: Option<String> = None;
                while i < args.len() {
                    if args[i] == "--test-mode" {
                        i += 1;
                        continue;
                    }
                    if args[i].starts_with('-') {
                        return cli_arg_error(&format!("unknown option '{}'", args[i]));
                    }
                    dest = Some(args[i].clone());
                    break;
                }
                if let Err(e) = cmd_std_install(dest.as_deref()) {
                    eprintln!("{}", e);
                    return 1;
                }
                return 0;
            }
            _ if args[i].starts_with('-') => {
                return cli_arg_error(&format!("unknown option '{}'", args[i]));
            }
            _ => {
                // First source/header is the input file.
                // `.h`/`.c` are required for `-c`/`--gen-cfc`; `.o` for `--link`.
                // Subsequent arguments are user program arguments (for JIT mode).
                if input_file.is_none()
                    && (args[i].ends_with(".cf")
                        || args[i].ends_with(".h")
                        || args[i].ends_with(".c")
                        || args[i].ends_with(".o"))
                {
                    input_file = Some(args[i].clone());
                } else {
                    user_program_args.push(args[i].clone());
                }
            }
        }
        i += 1;
    }

    let emit_kinds = [emit_ast, emit_llvm, emit_bc, emit_asm]
        .iter()
        .filter(|on| **on)
        .count();
    if emit_kinds > 1 {
        return cli_arg_error(
            "use only one of --emit-ast, --emit-llvm, --emit-bc, --emit-asm",
        );
    }

    if run_mode && test_cmd {
        return cli_arg_error("use either `coffee run` or `coffee test`, not both");
    }
    if run_mode {
        if jit_mode || emit_ast || emit_llvm || emit_bc || emit_asm || link_mode || gen_cfc {
            return cli_arg_error("`coffee run` compiles like --bin and executes; do not combine with --jit, --emit-*, --link, or -c");
        }
        bin_mode = true;
    }
    if test_cmd {
        if jit_mode || emit_ast || emit_llvm || emit_bc || emit_asm || link_mode || gen_cfc {
            return cli_arg_error("`coffee test` compiles like --bin and executes; do not combine with --jit, --emit-*, --link, or -c");
        }
        return cmd_test(
            input_file,
            opt_level,
            force_static,
            &static_libs,
            &include_paths,
            target_triple.as_ref(),
            enable_bitfields,
            enable_safety,
            show_memory,
            have_c,
        );
    }

    if link_mode {
        let Some(input_file) = input_file.as_deref() else {
            eprintln!("error: --link requires an object file (.o)");
            eprintln!("  = hint: compile a `.cf` source first, then `coffee --link <file.o>`");
            return 1;
        };
        if !input_file.ends_with(".o") {
            eprintln!("error: --link expected a `.o` file, got `{input_file}`");
            eprintln!("  = hint: pass the object file from a previous compile, not a source file");
            return 1;
        }
        let obj_path = Path::new(input_file);
        if !obj_path.exists() {
            eprintln!("error: object file not found: `{input_file}`");
            eprintln!("  = hint: compile the source with coffee (without --bin) to produce a `.o` first");
            return 1;
        }
        return link_object_files(
            &[input_file],
            &output_file,
            opt_level,
            target_triple.as_ref(),
        );
    }

    if gen_cfc {
        let Some(input_file) = input_file.as_deref() else {
            return cli_arg_error(
                "-c/--gen-cfc requires a .h or .c file (e.g. coffee -c header.h [-I path] -o libfoo.cfc)",
            );
        };
        if !input_file.ends_with(".h") && !input_file.ends_with(".c") {
            let error_kind = ErrorKind::CodeGeneration {
                stage: "input validation".to_string(),
                details: format!("{}: -c flag requires .h or .c file", input_file),
            };
            let diag = Diagnostic::new(
                Severity::Error,
                error_kind.clone(),
                format!(
                    "error: {} requires .h or .c file\n  = hint: coffee -c <header.h> [-I path] writes a .cfc",
                    input_file
                )
            );
            eprintln!("{}", diag.format());
            return 1;
        }

        use c::generator;
        match generator::generate_cfc(input_file, output_file.as_deref(), &include_paths) {
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

    // `--jit` / `--emit-ast` stay on the single-file driver (project mode always links).
    // `coffee run <file.cf>` is always single-file `--bin` + exec (tests live outside `src/`).
    // Project mode does not need a .cf on the command line (`coffee --emit-llvm`).
    if !jit_mode && !emit_ast && !(run_mode && input_file.is_some()) {
        if let Some(config_path) = detect_project_config() {
            if !run_mode {
                println!("Coffee project detected");
                println!("   Config: {}", config_path.display());
            }

            let entry_file = input_file.as_deref().filter(|s| s.ends_with(".cf"));

            let emit = if emit_llvm {
                compiler::EmitKind::LlvmIr
            } else if emit_bc {
                compiler::EmitKind::Bitcode
            } else if emit_asm {
                compiler::EmitKind::Assembly
            } else {
                compiler::EmitKind::Binary
            };

            let cli_opt = if opt_level_cli { Some(opt_level) } else { None };
            match compile_project(&config_path, entry_file, output_file.as_deref(), cli_opt, show_stats, target_triple.as_ref(), force_static, &static_libs, have_c, enable_bitfields, enable_safety, show_memory, emit) {
                Ok(artifact) => {
                    if run_mode {
                        return exec_program(&artifact, &user_program_args, false);
                    }
                    return 0;
                }
                Err(e) => {
                    eprintln!("{}", e);
                    return 1;
                }
            }
        }
    }

    if run_mode {
        let Some(input_file) = input_file.clone() else {
            return cli_arg_error("`coffee run` needs a .cf file, or a coffee.toml project in this directory");
        };
        let exe = match &output_file {
            Some(p) => PathBuf::from(p),
            None => temp_executable_path(&input_file),
        };
        let compile_status = compile_cf_to_native(
            &input_file,
            Some(exe.to_string_lossy().into_owned()),
            show_stats,
            show_memory,
            enable_bitfields,
            enable_safety,
            opt_level,
            force_static,
            &static_libs,
            true,
            &include_paths,
            target_triple.clone(),
            have_c,
            true,
        );
        if compile_status != 0 {
            cleanup_temp_exe(&exe);
            return compile_status;
        }
        return exec_program(&exe, &user_program_args, output_file.is_none());
    }

    let input_file = match input_file {
        Some(f) => f,
        None => {
            return cli_arg_error("missing input file (.cf)");
        }
    };

    compile_cf_file(
        &input_file,
        output_file,
        emit_ast,
        emit_llvm,
        emit_bc,
        emit_asm,
        show_stats,
        show_memory,
        enable_bitfields,
        enable_safety,
        opt_level,
        force_static,
        &static_libs,
        bin_mode,
        jit_mode,
        &include_paths,
        target_triple,
        have_c,
        &user_program_args,
        false,
    )
}


fn temp_executable_path(input_file: &str) -> PathBuf {
    let stem = Path::new(input_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    env::temp_dir().join(format!("coffee-run-{}-{}-{}", stem, process::id(), nanos))
}

fn cleanup_temp_exe(exe: &Path) {
    let _ = fs::remove_file(exe);
    let _ = fs::remove_file(PathBuf::from(format!("{}.o", exe.display())));
}

fn exec_program(exe: &Path, program_args: &[String], cleanup: bool) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(exe) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o111);
            let _ = fs::set_permissions(exe, perms);
        }
    }

    let status = Command::new(exe).args(program_args).status();
    let code = match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!(
                "error: failed to execute `{}`: {e}",
                exe.display()
            );
            1
        }
    };
    if cleanup {
        cleanup_temp_exe(exe);
    }
    code
}

/// Single-file `--bin` compile (same clang link path as the default driver).
fn compile_cf_to_native(
    input_file: &str,
    output_file: Option<String>,
    show_stats: bool,
    show_memory: bool,
    enable_bitfields: bool,
    enable_safety: bool,
    opt_level: u8,
    force_static: bool,
    static_libs: &[String],
    bin_mode: bool,
    include_paths: &[String],
    target_triple: Option<String>,
    have_c: bool,
    quiet: bool,
) -> i32 {
    compile_cf_file(
        input_file,
        output_file,
        false,
        false,
        false,
        false,
        show_stats,
        show_memory,
        enable_bitfields,
        enable_safety,
        opt_level,
        force_static,
        static_libs,
        bin_mode,
        false,
        include_paths,
        target_triple,
        have_c,
        &[],
        quiet,
    )
}

fn compile_cf_file(
    input_file: &str,
    output_file: Option<String>,
    emit_ast: bool,
    emit_llvm: bool,
    emit_bc: bool,
    emit_asm: bool,
    show_stats: bool,
    show_memory: bool,
    enable_bitfields: bool,
    enable_safety: bool,
    opt_level: u8,
    force_static: bool,
    static_libs: &[String],
    bin_mode: bool,
    jit_mode: bool,
    include_paths: &[String],
    target_triple: Option<String>,
    have_c: bool,
    user_program_args: &[String],
    quiet: bool,
) -> i32 {
    let validated_path = match validate_file_path(input_file) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };

    let source_code = match fs::read_to_string(&validated_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("error: failed to read source file: {}", e);
            return 1;
        }
    };

    let mut config = compiler::CompilerConfig::default();
    if let Err(e) = config.prepend_official_std() {
        eprintln!("{}", e);
        return 1;
    }
    for p in include_paths.iter().rev() {
        config.import_paths.insert(0, PathBuf::from(p));
    }
    let frontend = compiler::CompilerFrontend::with_config(config);
    let result = frontend.compile(&source_code, Some(input_file));

    if !result.success {
        for error in &result.errors {
            eprintln!("{}", error.format());
        }
        return 1;
    }

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

    if show_stats {
        println!("{}", frontend.format_result(&result));
    }

    if !result.imported_modules.is_empty() && show_stats {
        println!("  Imported modules:");
        for module in &result.imported_modules {
            println!("    - {}", module);
        }
    }

    if emit_ast {
        println!("// AST for {}", input_file);
        println!("// {} statements", result.stats.statements_parsed);
        for stmt in &result.program.statements {
            println!("{:?}", stmt);
        }
        return 0;
    }

    if jit_mode {
        match backend::compile_and_run(
            &result.program,
            &result.c_imports,
            result.cfc_symbols,
            result.hir_fns,
            user_program_args,
            backend::Backend::optimization_level(opt_level),
        ) {
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

    let output_path = output_file.unwrap_or_else(|| {
        let stem = Path::new(input_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("a");
        if emit_asm {
            format!("{}.s", stem)
        } else if emit_bc {
            format!("{}.bc", stem)
        } else if emit_llvm {
            format!("{}.ll", stem)
        } else if bin_mode {
            stem.to_string()
        } else {
            format!("{}.o", stem)
        }
    });

    let opt = backend::Backend::optimization_level(opt_level);

    let context = inkwell::context::Context::create();
    let mut llvm_backend = backend::Backend::with_target(&context, input_file, target_triple.clone());
    llvm_backend.set_opt_level(opt);

    let mut codegen = backend::codegen::CodeGenerator::new(&llvm_backend);
    codegen.enable_bitfields(enable_bitfields);
    codegen.enable_safety(enable_safety);
    if let Err(e) = codegen.compile_program_with_hir(
        &result.program,
        &result.c_imports,
        result.cfc_symbols,
        result.hir_fns,
    ) {
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

    if let Err(e) = llvm_backend.optimize(opt) {
        let error_kind = ErrorKind::CodeGeneration {
            stage: "LLVM optimization".to_string(),
            details: e,
        };
        let diag = Diagnostic::new(
            Severity::Error,
            error_kind.clone(),
            error_kind.description(),
        );
        eprintln!("{}", diag.format());
        return 1;
    }

    let requested = PathBuf::from(&output_path);
    let bin_link = !emit_asm && !emit_llvm && !emit_bc && bin_mode;
    let write_path = if bin_link {
        PathBuf::from(format!("{}.o", requested.display()))
    } else {
        requested.clone()
    };

    let write_result = if emit_asm {
        llvm_backend.write_assembly_file(&write_path)
    } else if emit_bc {
        llvm_backend.write_bitcode(&write_path)
    } else if emit_llvm {
        llvm_backend.write_ir(&write_path)
    } else {
        llvm_backend.write_object_file(&write_path)
    };

    match write_result {
        Ok(_) => {
            if codegen.has_structs() {
                println!("{}", codegen.get_memory_layout_report());
            }

            if have_c {
                let functions: Vec<coffee::parser::function::Function> = result.program.statements.iter()
                    .filter_map(|stmt| {
                        if let coffee::parser::Statement::Function(f) = stmt {
                            Some(f.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                let classes: Vec<coffee::parser::class::ClassDef> = result.program.statements.iter()
                    .filter_map(|stmt| {
                        if let coffee::parser::Statement::Class(c) = stmt {
                            Some(c.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                let header_output_dir = Path::new(input_file).parent().unwrap_or(Path::new("."));
                let header_name = Path::new(input_file).file_stem().unwrap().to_str().unwrap();
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

            let has_main = llvm_backend.module.get_function("main").is_some();
            if !has_main && !quiet {
                eprintln!("Warning: This module does not contain a main() entry point.");
                eprintln!("  = note: It cannot be executed directly as a program.");
                eprintln!("  = note: This module can be used as a library (imported by other modules).");
                eprintln!("  = help: To make it executable, add a main statement:");
                eprintln!("        main(entry_function())");
                eprintln!("  = help: With command-line arguments:");
                eprintln!("        main(entry_function(arg1, arg2))");
            }

            if bin_link {
                let exe_path = requested.clone();
                let mut link_opts = compiler::linker::link_options_from_c_imports(
                    &result.c_imports,
                    force_static,
                    static_libs,
                    opt_level,
                    target_triple.clone(),
                );
                if let Err(details) = apply_c_import_library_search(
                    &mut link_opts,
                    &result.c_imports,
                    force_static,
                ) {
                    let error_kind = ErrorKind::LinkError { details };
                    let diag = Diagnostic::new(
                        Severity::Error,
                        error_kind.clone(),
                        error_kind.description(),
                    );
                    eprintln!("{}", diag.format());
                    return 1;
                }

                let linker = compiler::linker::Linker::with_options(exe_path.clone(), link_opts);
                match linker.link(&[write_path.clone()]) {
                    Ok(()) => {
                        if !quiet {
                            println!("Compiled {} -> {}", input_file, exe_path.display());
                        }

                        if show_memory && codegen.has_structs() {
                            println!("\n=== Memory Layout Report ===\n");
                            println!("{}", codegen.get_memory_layout_report());
                        }
                    }
                    Err(details) => {
                        let error_kind = ErrorKind::LinkError { details };
                        let diag = Diagnostic::new(
                            Severity::Error,
                            error_kind.clone(),
                            error_kind.description(),
                        );
                        eprintln!("{}", diag.format());
                        return 1;
                    }
                }
            } else {
                if !quiet {
                    println!("Compiled {} -> {}", input_file, write_path.display());
                }

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

    0
}

fn discover_test_files(project_root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let tests_dir = project_root.join("tests");
    if tests_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&tests_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("cf") {
                    files.push(path);
                }
            }
        }
    }
    if files.is_empty() {
        if let Ok(entries) = fs::read_dir(project_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with("test_") && name.ends_with(".cf") {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

fn cmd_test(
    input_file: Option<String>,
    opt_level: u8,
    force_static: bool,
    static_libs: &[String],
    include_paths: &[String],
    target_triple: Option<&String>,
    enable_bitfields: bool,
    enable_safety: bool,
    show_memory: bool,
    have_c: bool,
) -> i32 {
    let files: Vec<PathBuf> = if let Some(f) = input_file {
        vec![PathBuf::from(f)]
    } else {
        let Some(config_path) = detect_project_config() else {
            eprintln!("error: `coffee test` needs a .cf file, or a coffee.toml project");
            eprintln!("  = hint: `coffee test foo.cf` in single-file mode, or `coffee test` in a project with tests/*.cf");
            return 1;
        };
        let root = config_path.parent().unwrap_or(Path::new("."));
        let found = discover_test_files(root);
        if found.is_empty() {
            eprintln!(
                "error: no test files found under `{}` (looked for tests/*.cf, then test_*.cf)",
                root.display()
            );
            return 1;
        }
        found
    };

    println!("running {} test{}", files.len(), if files.len() == 1 { "" } else { "s" });
    let mut passed = 0usize;
    let mut failed = 0usize;
    for file in &files {
        let display = file.display().to_string();
        let exe = temp_executable_path(&display);
        let compile_status = compile_cf_to_native(
            &display,
            Some(exe.to_string_lossy().into_owned()),
            false,
            show_memory,
            enable_bitfields,
            enable_safety,
            opt_level,
            force_static,
            static_libs,
            true,
            include_paths,
            target_triple.cloned(),
            have_c,
            true,
        );
        if compile_status != 0 {
            println!("    {display} ... FAILED (compile)");
            cleanup_temp_exe(&exe);
            failed += 1;
            continue;
        }
        let code = exec_program(&exe, &[], true);
        if code == 0 {
            println!("    {display} ... ok");
            passed += 1;
        } else {
            println!("    {display} ... FAILED (exit {code})");
            failed += 1;
        }
    }

    println!();
    if failed == 0 {
        println!("test result: ok. {passed} passed; 0 failed");
        0
    } else {
        println!("test result: FAILED. {passed} passed; {failed} failed");
        1
    }
}

/// Link object files to create an executable
/// Returns exit code (0 = success, 1 = failure)
fn link_object_files(
    object_files: &[&str],
    output_file: &Option<String>,
    opt_level: u8,
    target_triple: Option<&String>,
) -> i32 {
    let output_path: PathBuf = if let Some(out) = output_file {
        PathBuf::from(out)
    } else {
        // Default: use first object file name without .o extension
        let first_obj = Path::new(&object_files[0]);
        first_obj.with_extension("")
    };

    let objs: Vec<PathBuf> = object_files.iter().map(PathBuf::from).collect();
    let opts = compiler::linker::LinkOptions {
        opt_level,
        target_triple: target_triple.cloned(),
        ..Default::default()
    };
    let linker = compiler::linker::Linker::with_options(output_path.clone(), opts);
    match linker.link(&objs) {
        Ok(()) => {
            println!(
                "Linked {} -> {}",
                object_files.join(" "),
                output_path.display()
            );
            0
        }
        Err(details) => {
            let error_kind = ErrorKind::LinkError { details };
            let diag = Diagnostic::new(
                Severity::Error,
                error_kind.clone(),
                error_kind.description(),
            );
            eprintln!("{}", diag.format());
            1
        }
    }
}

fn cmd_fetch(rest: &[String]) -> Result<(), String> {
    let mut url: Option<String> = None;
    let mut save = false;
    let mut save_name: Option<String> = None;
    let mut force = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--test-mode" => {}
            "--force" => force = true,
            "--save" => {
                save = true;
                if i + 1 < rest.len() {
                    let next = rest[i + 1].as_str();
                    if !next.starts_with('-')
                        && !next.contains("://")
                        && !next.ends_with(".tar.gz")
                    {
                        save_name = Some(next.to_string());
                        i += 1;
                    }
                }
            }
            arg if arg.starts_with('-') => {
                return Err(format!("unknown fetch option '{arg}'"));
            }
            arg => {
                if url.is_some() {
                    return Err("fetch accepts at most one URL".to_string());
                }
                url = Some(arg.to_string());
            }
        }
        i += 1;
    }

    if let Some(url) = url {
        let (root, hash) = compiler::pkg::fetch_url(&url, None)?;
        println!("hash = \"{hash}\"");
        println!("cached: {}", root.display());
        if save {
            let toml_path = find_coffee_toml().ok_or_else(|| {
                "coffee fetch --save requires a coffee.toml (run from a project)".to_string()
            })?;
            let key = if let Some(name) = save_name {
                name
            } else {
                let pkg = compiler::ProjectConfig::from_package_file(&root.join("coffee.toml"))?;
                pkg.package.name
            };
            compiler::pkg::save_dep_to_toml(&toml_path, &key, &url, &hash, force)?;
            println!("saved [dependencies.packages.{key}] in {}", toml_path.display());
        }
        return Ok(());
    }

    if save {
        return Err("coffee fetch --save requires a URL".to_string());
    }

    let toml_path = find_coffee_toml().ok_or_else(|| {
        "coffee fetch requires a coffee.toml in this directory or a parent".to_string()
    })?;
    let config = compiler::ProjectConfig::from_file(&toml_path)?;
    compiler::pkg::fetch_project_deps(&config)?;
    println!("fetch complete");
    Ok(())
}

fn cmd_std_install(dest_arg: Option<&str>) -> Result<(), String> {
    let dest = if let Some(d) = dest_arg {
        PathBuf::from(d)
    } else {
        compiler::pkg::default_std_install_dir()?
    };
    compiler::pkg::extract_embedded_std(&dest)?;
    let abs = dest.canonicalize().unwrap_or_else(|_| dest.clone());
    println!("{}", abs.display());
    Ok(())
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
            "error: cannot initialize project: directory `{}` is not empty\n  = note: `{}` already exists\n  = hint: use a new directory, or remove coffee.toml first",
            cwd.display(),
            config_path.display()
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
        fs::create_dir(&src_dir).map_err(|e| init_write_error(&src_dir, e))?;
        println!("  Created src/");
    }

    if !lib_dir.exists() {
        fs::create_dir(&lib_dir).map_err(|e| init_write_error(&lib_dir, e))?;
        println!("  Created lib/");
    }

    if !target_dir.exists() {
        fs::create_dir(&target_dir).map_err(|e| init_write_error(&target_dir, e))?;
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

# Official std is bundled with the compiler (no path= here).
# If compile says std is missing, run: coffee std install
#
# Package deps: `path` XOR (`url` + tree `hash`).
# [dependencies.packages.local_example]
# path = "../other_pkg"
#
# [dependencies.packages.remote_example]
# url = "https://example.com/pkg.tar.gz"
# hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

# C 库：声明后编译会从 headers 生成 target/cfc（系统里已安装的库才能找到头和 .so）
# [dependencies.c_libraries.z]
# headers = ["zlib.h"]
# name 可省略（默认等于键 z → -lz）
#
# [dependencies.c_libraries.libm]
# name = "m"
# headers = ["math.h"]
# static_link = false

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

    fs::write(&config_path, config_content).map_err(|e| init_write_error(&config_path, e))?;
    println!("  Created coffee.toml");

    // Create src/main.cf
    println!("Creating src/main.cf...");
    let main_content = r#"use print in std
fn main() => int:
    print("Hello, Coffee!\n")
    return 0
"#;

    let main_path = src_dir.join("main.cf");
    fs::write(&main_path, main_content).map_err(|e| init_write_error(&main_path, e))?;
    println!("  Created src/main.cf");

    // Create lib/.gitkeep
    let gitkeep_path = lib_dir.join(".gitkeep");
    fs::write(&gitkeep_path, "").map_err(|e| init_write_error(&gitkeep_path, e))?;

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

fn init_write_error(path: &Path, err: std::io::Error) -> String {
    format!(
        "error: failed to write `{}`: {err}\n  = hint: check directory permissions and disk space",
        path.display()
    )
}

fn cli_arg_error(message: &str) -> i32 {
    eprintln!("error: {message}");
    eprintln!("Usage: coffee [options] <input.cf>");
    eprintln!("Try 'coffee --help' for more information.");
    1
}

fn print_usage() {
    println!("Coffee Compiler v{}", compiler::compiler_display_version());
    println!();
    println!("Usage: coffee [options] <input.cf>");
    println!("   or: coffee run [options] [file.cf] [-- args...]");
    println!("   or: coffee test [options] [file.cf]");
    println!("   or: coffee init         Initialize a new Coffee project");
    println!("   or: coffee fetch        Fetch package dependencies into the cache");
    println!("   or: coffee std install  Install the bundled standard library");
    println!();
    println!("Commands:");
    println!("  run [file.cf] [-- args] Compile like --bin to a temp exe and run it");
    println!("                          (project entry if coffee.toml and no file)");
    println!("  test [file.cf]          Compile+run tests/*.cf (or test_*.cf), or one file");
    println!("  init                    Initialize a new Coffee project in current directory");
    println!("  std install [dir]       Extract bundled std (COFFEE_STD / XDG / ~/.local/share/coffee/std)");
    println!("  fetch                   Fetch url+hash deps of the nearest coffee.toml");
    println!("  fetch <url>             Download a .tar.gz, print tree hash, cache it");
    println!("  fetch <url> --save [name]  Same, and write [dependencies.packages.<name>]");
    println!("                          (name defaults to the fetched [package].name)");
    println!("  fetch <url> --save --force  Overwrite an existing package key");
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
    println!("  --have-c                Emit a C header for Coffee `c fn` exports");
    println!("  --jit, -j               JIT execution mode");
    println!("  --test-mode             Skip the process lock (for the test harness)");
    println!("  --target <triple>       Target triple for cross-compilation");
    println!("                          (e.g., x86_64-unknown-linux-gnu, aarch64-linux-android)");
    println!("  --static                Force static linking for all C libraries");
    println!("  --static-lib <name>     Link specific library statically");
    println!("  -O0, -O1, -O2, -O3      Set optimization level");
    println!("  -I <path>               Add include path for .cfc");
    println!("  -o <file>               Output path (executable with --bin; else .o / emit file)");
    println!();
    println!("Default (no --bin / --emit-*): write a .o object file.");
}

