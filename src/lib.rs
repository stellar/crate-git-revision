//! Embed the git revision of a crate in its build.
//!
//! Supports embedding the version from a local or remote git repository the build
//! is occurring in, as well as when `cargo install` or depending on a crate
//! published to crates.io.
//!
//! It extracts the git revision in two ways:
//! - From the `.cargo_vcs_info.json` file embedded in published crates.
//! - From the git repository the build is occurring from in unpublished crates.
//!
//! Injects an environment variable `GIT_REVISION` into the build that contains
//! the full git revision, with a `-dirty` suffix if the working directory is
//! dirty.
//!
//! For example, for a clean worktree:
//!
//! ```text
//! 1a2b3c4d5e6f7890abcdef1234567890abcdef12
//! ```
//!
//! For example, suffixed with `-dirty` when a worktree contains changes:
//!
//! ```text
//! 1a2b3c4d5e6f7890abcdef1234567890abcdef12-dirty
//! ```
//!
//! The dirty check uses `git status --porcelain`, which also reports
//! submodule state changes. Crates that vendor submodules may produce
//! `-dirty` builds where they did not previously when the submodule's
//! working tree differs from its recorded commit.
//!
//! ### Untracked files are not considered dirty
//!
//! Only changes to tracked files mark the revision as `-dirty`. Untracked
//! files in the worktree are intentionally ignored (`git status` is invoked
//! with `--untracked-files=no`).
//!
//! Detecting untracked files reliably from a build script would require
//! telling Cargo to re-run `build.rs` whenever any file appears anywhere in
//! the crate directory (e.g. `cargo:rerun-if-changed=.`). That makes the
//! build script — and everything that depends on it — re-run on essentially
//! every filesystem change in the tree, including editor swap files and
//! unrelated edits. The cost on every incremental build outweighs the
//! marginal correctness gain, especially since untracked files do not
//! typically affect a Rust build: source files are pulled into a crate
//! explicitly via `mod` declarations and `#[path]` attributes, not by
//! scanning the filesystem.
//!
//! Without watching the whole tree, the dirty signal for untracked files
//! would be inconsistent: a cached build would report clean even after an
//! untracked file appeared, and only `cargo clean` followed by a fresh
//! build would surface it. To avoid that inconsistency, untracked files
//! are not part of the dirty check at all.
//!
//! Requires the use of a build.rs build script. See [Build Scripts]() for more
//! details on how Rust build scripts work.
//!
//! [Build Scripts]: https://doc.rust-lang.org/cargo/reference/build-scripts.html
//!
//! ### Examples
//!
//! Add the following to the crate's `Cargo.toml` file:
//!
//! ```toml
//! [build_dependencies]
//! crate-git-revision = "0.0.2"
//! ```
//!
//! Add the following to the crate's `build.rs` file:
//!
//! ```rust
//! crate_git_revision::init();
//! ```
//!
//! Add the following to the crate's `lib.rs` or `main.rs` file:
//!
//! ```ignore
//! pub const GIT_REVISION: &str = env!("GIT_REVISION");
//! ```

use std::{fs::read_to_string, path::Path, process::Command, str};

/// Initialize the GIT_REVISION environment variable with the git revision of
/// the current crate.
///
/// Intended to be called from within a build script, `build.rs` file, for the
/// crate.
pub fn init() {
    let _res = __init(&mut std::io::stdout(), &std::env::current_dir().unwrap());
}

fn __init(w: &mut impl std::io::Write, current_dir: &Path) -> std::io::Result<()> {
    let mut git_sha: Option<String> = None;

    // Read the git revision from the JSON file embedded by cargo publish. This
    // will get the version from published crates.
    if let Ok(vcs_info) = read_to_string(current_dir.join(".cargo_vcs_info.json")) {
        let vcs_info: Result<CargoVcsInfo, _> = serde_json::from_str(&vcs_info);
        if let Ok(vcs_info) = vcs_info {
            git_sha = Some(vcs_info.git.sha1.trim().to_string());
        }
    }

    // Read the git revision from the git repository containing the code being
    // built.
    if git_sha.is_none() {
        match Command::new("git")
            .current_dir(current_dir)
            .arg("rev-parse")
            .arg("--git-dir")
            .output()
            .map(|o| o.stdout)
        {
            Err(e) => {
                writeln!(
                    w,
                    "cargo:warning=Error getting git directory to get git revision: {e:?}"
                )?;
            }
            Ok(git_dir) => {
                let git_dir = String::from_utf8_lossy(&git_dir);
                let git_dir = git_dir.trim();

                // Require the build script to rerun if relavent git state changes which
                // changes the current git commit.
                //  - .git/index: Changes if the index/staged files changes, which will
                //  cause the repo to be dirty.
                //  - .git/HEAD: Changes if the ref currently in the working directory,
                //  and potentially the commit, to change.
                //  - .git/refs: Changes to any files in refs could cause the current
                //  commit to have changed if the ref in .git/HEAD is changed.
                // Note: That changes in the above files may not result in material
                // changes to the crate, but changes in any should invalidate the
                // revision since the revision can be changed by any of the above.
                writeln!(w, "cargo:rerun-if-changed={git_dir}/index")?;
                writeln!(w, "cargo:rerun-if-changed={git_dir}/HEAD")?;
                writeln!(w, "cargo:rerun-if-changed={git_dir}/refs")?;

                match Command::new("git")
                    .current_dir(current_dir)
                    .arg("rev-parse")
                    .arg("HEAD")
                    .output()
                    .map(|o| o.stdout)
                {
                    Err(e) => {
                        writeln!(
                            w,
                            "cargo:warning=Error getting git revision from {current_dir:?}: {e:?}"
                        )?;
                    }
                    Ok(rev_parse) => {
                        let sha = str::from_utf8(&rev_parse)
                            .ok()
                            .map(|s| s.trim().to_string());
                        if let Some(sha) = sha.filter(|s| !s.is_empty()) {
                            let dirty = match Command::new("git")
                                .current_dir(current_dir)
                                .arg("status")
                                .arg("--porcelain")
                                .arg("--untracked-files=no")
                                .output()
                                .map(|o| o.stdout)
                            {
                                Ok(status) => !status.is_empty(),
                                Err(e) => {
                                    writeln!(
                                        w,
                                        "cargo:warning=Error checking git dirty status from {current_dir:?}, marking as dirty: {e:?}"
                                    )?;
                                    true
                                }
                            };
                            git_sha = Some(if dirty { format!("{sha}-dirty") } else { sha });
                        }
                    }
                }
            }
        }
    }

    if let Some(git_sha) = git_sha.filter(|s| !s.is_empty()) {
        writeln!(w, "cargo:rustc-env=GIT_REVISION={git_sha}")?;
    }

    Ok(())
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Default)]
struct CargoVcsInfo {
    git: CargoVcsInfoGit,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Default)]
struct CargoVcsInfoGit {
    sha1: String,
}

mod test;
