# crate-git-revision

Embed the git revision of a crate in its build.

Supports embedding the version from a local or remote git repository the build
is occurring in, as well as when `cargo install` or depending on a crate
published to crates.io.

It extracts the git revision in two ways:
- From the `.cargo_vcs_info.json` file embedded in published crates.
- From the git repository the build is occurring from in unpublished crates.

Injects an environment variable `GIT_REVISION` into the build that contains
the full git revision, with a `-dirty` suffix if the working directory is
dirty.

For example, for a clean worktree:

```text
1a2b3c4d5e6f7890abcdef1234567890abcdef12
```

For example, suffixed with `-dirty` when a worktree contains changes:

```text
1a2b3c4d5e6f7890abcdef1234567890abcdef12-dirty
```

The dirty check uses `git status --porcelain`, which also reports submodule
state changes. Crates that vendor submodules may produce `-dirty` builds
where they did not previously when the submodule's working tree differs
from its recorded commit.

### Untracked files are not considered dirty

Only changes to tracked files mark the revision as `-dirty`. Untracked
files in the worktree are intentionally ignored (`git status` is invoked
with `--untracked-files=no`).

Detecting untracked files reliably from a build script would require
telling Cargo to re-run `build.rs` whenever any file appears anywhere in
the crate directory (e.g. `cargo:rerun-if-changed=.`). That makes the
build script — and everything that depends on it — re-run on essentially
every filesystem change in the tree, including editor swap files and
unrelated edits. The cost on every incremental build outweighs the
marginal correctness gain, especially since untracked files do not
typically affect a Rust build: source files are pulled into a crate
explicitly via `mod` declarations and `#[path]` attributes, not by
scanning the filesystem.

Without watching the whole tree, the dirty signal for untracked files
would be inconsistent: a cached build would report clean even after an
untracked file appeared, and only `cargo clean` followed by a fresh
build would surface it. To avoid that inconsistency, untracked files
are not part of the dirty check at all.

Requires the use of a build.rs build script. See [Build Scripts]() for more
details on how Rust build scripts work.

[Build Scripts]: https://doc.rust-lang.org/cargo/reference/build-scripts.html

#### Examples

Add the following to the crate's `Cargo.toml` file:

```toml
[build_dependencies]
crate-git-revision = "0.0.2"
```

Add the following to the crate's `build.rs` file:

```rust
crate_git_revision::init();
```

Add the following to the crate's `lib.rs` or `main.rs` file:

```rust
pub const GIT_REVISION: &str = env!("GIT_REVISION");
```

License: Apache-2.0
