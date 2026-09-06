# Close remaining health-map gaps

**Status (2026-09-05):** Plan B memory contract, dual-frontend unify, literal narrowing, and intra-procedural borrow checking have landed. Remaining out-of-scope items: full nom rewrite of `expr.rs`; lifetime parameters / field places; new C-handle ownership spec beyond `object`.

Do not change `mv`/`clone`/`rm` language meaning. `int(N)+` is bytes. No try/catch. Cargo: `flock /data/data/com.termux/files/home/coffee/.superpowers/sdd/cargo.lock cargo test --offline … -- --test-threads=1`.
