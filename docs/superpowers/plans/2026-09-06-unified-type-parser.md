# Unified Type Parser Implementation Plan

> executing-plans. No commit. No version bump.

**Goal:** One balanced `parse_type`; class/fn `type_params`.

**Files:** Create `src/parser/ty.rs`. Modify `src/parser/mod.rs`, `function.rs`, `var.rs`, `class/mod.rs`, `class/parse_class.rs`, `class/def.rs`, `function.rs` `Function` struct, every `ClassDef {` / `Function {` that needs the new field (grep the repo).

- [ ] Tests first in `src/parser/ty.rs` `#[cfg(test)]` and class/function parse tests.
- [ ] Implement scanner + wire replacements.
- [ ] `type_params` on ClassDef/Function; parse `<T, U>` after name before `of` / `(`.
- [ ] `flock /data/data/com.termux/files/home/coffee/.superpowers/sdd/cargo.lock cargo test --offline --lib parser:: -- --test-threads=1`

Do not change `reject_fake_generics`. Do not touch `src/backend`.
