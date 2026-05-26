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
/// crate. Uses `CARGO_MANIFEST_DIR` to locate the crate, which is set by Cargo
/// when executing the build script.
pub fn init() {
    // Use CARGO_MANIFEST_DIR rather than the current directory so the crate
    // path is correct even when `package.build` points to a build script
    // outside the package source tree.
    // https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-build-scripts
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR is set by cargo when executing build scripts");
    let _res = __init(&mut std::io::stdout(), Path::new(&manifest_dir));
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
        match git(current_dir)
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

                match git(current_dir)
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
                            let dirty = match git(current_dir)
                                .arg("status")
                                .arg("--porcelain")
                                .arg("--untracked-files=no")
                                .output()
                            {
                                Ok(output) if output.status.success() => !output.stdout.is_empty(),
                                Ok(output) => {
                                    writeln!(
                                        w,
                                        "cargo:warning=Error checking git dirty status from {current_dir:?}, marking as dirty: git status exited with {status}",
                                        status = output.status,
                                    )?;
                                    true
                                }
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

// Build a `git` Command that ignores ambient path-redirecting GIT_* env vars,
// so that the recorded revision is always for the repository at current_dir
// and not whatever an outer process (e.g. `git rebase --exec cargo ...`) has
// pointed git at. Mirrors the sanitization cargo applies in fetch_with_cli.
fn git(current_dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(current_dir);
    cmd.env_remove("GIT_DIR");
    cmd.env_remove("GIT_WORK_TREE");
    cmd.env_remove("GIT_INDEX_FILE");
    cmd.env_remove("GIT_OBJECT_DIRECTORY");
    cmd.env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES");
    // Disable terminal prompts so a misconfigured credential or hook can't
    // hang a non-interactive build (e.g. CI) indefinitely.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd
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
