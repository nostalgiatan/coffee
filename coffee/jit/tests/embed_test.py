"""The JIT as a shared library, used from Python with ctypes: the host function is a Python callback.
usage: embed_test.py libcfjit.so ema1.cir emb.cir"""
import ctypes, sys

lib = ctypes.CDLL(sys.argv[1])
lib.cfjit_new.restype = ctypes.c_long
lib.cfjit_register.argtypes = [ctypes.c_long, ctypes.c_char_p, ctypes.c_void_p]
lib.cfjit_load.argtypes = [ctypes.c_long, ctypes.c_char_p, ctypes.c_long, ctypes.c_char_p, ctypes.c_long]
lib.cfjit_load.restype = ctypes.c_long
lib.cfjit_sym.argtypes = [ctypes.c_long, ctypes.c_char_p]
lib.cfjit_sym.restype = ctypes.c_void_p

HOST = ctypes.CFUNCTYPE(ctypes.c_long, ctypes.c_long)
seen = []
def scale(x):
    seen.append(x)
    return x * 10
cb = HOST(scale)          # kept alive by this name

ctx = lib.cfjit_new()
assert ctx > 0
assert lib.cfjit_register(ctx, b"host_scale", ctypes.cast(cb, ctypes.c_void_p)) == 0
err = ctypes.create_string_buffer(200)
for path in sys.argv[2:]:
    data = open(path, "rb").read()
    assert lib.cfjit_load(ctx, data, len(data), err, 200) > 0, err.value
run = ctypes.CFUNCTYPE(ctypes.c_long, ctypes.c_long)(lib.cfjit_sym(ctx, b"run"))
assert run(3) == 62, run(3)
assert seen == [3], seen
print("ok")
