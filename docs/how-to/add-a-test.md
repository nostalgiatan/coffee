# 如何添加测试

目标:给某个阶段加一条能抓住回归的测试。

## 放在哪里

在该阶段的 `tests/` 下新建 `test_<主题>.cf`,或者在现有文件的 `main` 里追加。每个文件是一个独立的程序,`main` 返回失败的个数。

## 写一条运行类测试(coffee/ir)

`ir/src/drive.cf` 提供辅助函数。`expect_exit(源码, 期望退出码)` 把源码编译、链接、运行,比较退出码,返回 0 或 1:

```
use * in drive

fn main() => int:
    let m = 0
    m = m + expect_exit("fn main() => int:\n    return 3\n", 3)
    return m
```

需要机器码后端时用 `expect_mc`,它不看标志文件。程序在中途退出时(例如越界检查失败)内存不会释放,改用 `expect_exit_early`。

退出码只有低 8 位。期望值写 `318` 时实际得到 `62`。

## 写一条优化器测试

手工构造 IR,调用 `optimize`,再数指令。`tests/test_opt.cf` 里有 `count_op(f, 操作码)` 和 `sound(编号, f)`(检查验证器没有报错)可以直接用:

```
let f = new_fun("name", int(Rp.I64))
let b = f.new_block()
f.set_block(b)
let p = f.alloc(16)
f.free_n(p, 16)
f.ret(f.cint(int(Rp.I64), 0))
optimize(&f)
m = m + expect(1, count_op(&f, int(Op.Alloc)), 0)
```

## 确认测试真的有用

改坏被测的代码,测试应该失败,然后改回来。没改坏也通过的测试什么都没测。

## 相关

- 运行方式:[如何运行测试](run-the-tests.md)
