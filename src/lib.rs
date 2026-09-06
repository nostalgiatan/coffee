// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License
//
// Coffee compiler library (frontend + backend). The `coffee` binary is `src/main.rs`.

pub mod parser;
pub mod types;
pub mod semantic;
pub mod compiler;
pub mod diagnostics;
pub mod backend;
pub mod c;
pub mod library_finder;
pub mod hir;
#[macro_use]
pub mod debug_log;
