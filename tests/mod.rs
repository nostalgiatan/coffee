// Coffee编译器集成测试
// 
// 测试内容包括：
// - 类的定义和使用
// - 纯数据结构类（栈分配）
// - 完整类（堆分配）
// - 构造函数调用
// - 方法调用
// - 内存管理
// - 基础类型
// - 表达式
// - 控制流
// - 函数
// - 导入
// - 异常处理
// - 模式匹配

mod common;

// 包含所有测试模块
mod class_tests;
mod method_tests;
mod memory_tests;
mod constructor_tests;
mod basic_types_tests;
mod expressions_tests;
mod control_flow_tests;
mod functions_tests;
mod imports_tests;
mod error_handling_tests;
mod pattern_matching_tests;