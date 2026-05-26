#![cfg(test)]

use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::str;

fn init_git_repo(path: &Path) {
    let output = Command::new("git")
        .current_dir(path)
        .arg("init")
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = Command::new("git")
        .current_dir(path)
        .arg("config")
        .arg("user.email")
        .arg("whatever@example.com")
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = Command::new("git")
        .current_dir(path)
        .arg("config")
        .arg("user.name")
        .arg("whatever")
        .output()
        .unwrap();
    assert!(output.status.success());

    let file = path.join("readme");
    fs::write(file, "hello").unwrap();

    let output = Command::new("git")
        .current_dir(path)
        .arg("add")
        .arg("readme")
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = Command::new("git")
        .current_dir(path)
        .arg("commit")
        .arg("-am")
        .arg("test")
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn test_init() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    let mut out = Vec::new();
    let res = super::__init(&mut out, git_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rerun-if-changed=.git/index
cargo:rerun-if-changed=.git/HEAD
cargo:rerun-if-changed=.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]{40}\n";
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
}

#[test]
fn test_init_subdir() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    let manifest_dir = git_dir.join("subdir");
    std::fs::create_dir(&manifest_dir).unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, &manifest_dir);
    assert!(res.is_ok());

    // In the subdir case, `git rev-parse --git-dir` returns an absolute path
    // with symlinks resolved. This shows up on macs where the tempdir includes
    // a symlink to /private/var.
    let git_dir = std::fs::canonicalize(git_dir).unwrap();

    let out = str::from_utf8(&out).unwrap();
    let expected = &format!(
        "cargo:rerun-if-changed={gd}/.git/index
cargo:rerun-if-changed={gd}/.git/HEAD
cargo:rerun-if-changed={gd}/.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]{{40}}\n",
        gd = git_dir.display()
    );
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
}

#[test]
fn test_dirty() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    let file = git_dir.join("readme");
    fs::write(file, "dirty").unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, git_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rerun-if-changed=.git/index
cargo:rerun-if-changed=.git/HEAD
cargo:rerun-if-changed=.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]{40}-dirty\n";
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
}

#[test]
fn test_untracked_is_not_dirty() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    let file = git_dir.join("new-file");
    fs::write(file, "untracked").unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, git_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rerun-if-changed=.git/index
cargo:rerun-if-changed=.git/HEAD
cargo:rerun-if-changed=.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]{40}\n";
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
    assert!(!out.contains("-dirty"));
}

#[test]
fn test_dirty_when_status_fails() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    // Corrupt the index so `git status` fails with a non-zero exit but
    // `git rev-parse HEAD` still succeeds.
    fs::write(git_dir.join(".git/index"), "garbage").unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, git_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rerun-if-changed=.git/index
cargo:rerun-if-changed=.git/HEAD
cargo:rerun-if-changed=.git/refs
cargo:warning=Error checking git dirty status from .*, marking as dirty: .*
cargo:rustc-env=GIT_REVISION=[0-9a-f]{40}-dirty\n";
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
}

#[test]
fn test_init_with_tag_does_not_use_describe() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    let output = Command::new("git")
        .current_dir(git_dir)
        .arg("tag")
        .arg("-a")
        .arg("v1.0.0")
        .arg("-m")
        .arg("tag")
        .output()
        .unwrap();
    assert!(output.status.success());

    let file = git_dir.join("readme");
    fs::write(file, "second").unwrap();
    let output = Command::new("git")
        .current_dir(git_dir)
        .arg("commit")
        .arg("-am")
        .arg("second")
        .output()
        .unwrap();
    assert!(output.status.success());

    let mut out = Vec::new();
    let res = super::__init(&mut out, git_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rerun-if-changed=.git/index
cargo:rerun-if-changed=.git/HEAD
cargo:rerun-if-changed=.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]{40}\n";
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
    assert!(!out.contains("v1.0.0"));
}

#[test]
fn test_published() {
    let tempdir = tempfile::tempdir().unwrap();
    let crate_dir = tempdir.path();

    let vcs_info = r#"{
  "git": {
    "sha1": "0c5255b6f47649305fcb68edccb285510aec71a7"
  },
  "path_in_vcs": ""
}"#;

    let file = crate_dir.join(".cargo_vcs_info.json");
    fs::write(file, vcs_info).unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, crate_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rustc-env=GIT_REVISION=0c5255b6f47649305fcb68edccb285510aec71a7\n";
    println!("{out}");
    println!("{expected}");
    assert_eq!(out, expected);
}

#[test]
fn test_published_empty_sha() {
    let tempdir = tempfile::tempdir().unwrap();
    let crate_dir = tempdir.path();

    let vcs_info = r#"{
  "git": {
    "sha1": ""
  },
  "path_in_vcs": ""
}"#;

    let file = crate_dir.join(".cargo_vcs_info.json");
    fs::write(file, vcs_info).unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, crate_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:warning=GIT_REVISION not set\n";
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
    assert!(!out.contains("cargo:rustc-env=GIT_REVISION="));
}

#[test]
fn test_warns_when_revision_cannot_be_derived() {
    // A directory that is neither a git checkout nor a published crate
    // (no .cargo_vcs_info.json) should not set GIT_REVISION and should
    // emit a cargo:warning so the missing value is visible.
    let tempdir = tempfile::tempdir().unwrap();
    let crate_dir = tempdir.path();

    let mut out = Vec::new();
    let res = super::__init(&mut out, crate_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    println!("{out}");
    assert!(out.contains("cargo:warning=GIT_REVISION not set"));
    assert!(!out.contains("cargo:rustc-env=GIT_REVISION="));
}

// Verifies that ambient GIT_* env vars in the parent process do not
// influence __init. Done by re-invoking the test binary as a subprocess
// with adversarial env vars set on the child, so the env modifications
// never touch the parent test process and cannot race with parallel tests.
#[test]
fn test_ambient_git_env_vars_are_ignored() {
    const CHILD_REPO_ENV: &str = "CRATE_GIT_REVISION_TEST_CHILD_REPO";

    if let Some(git_dir) = std::env::var_os(CHILD_REPO_ENV) {
        let mut out = Vec::new();
        super::__init(&mut out, Path::new(&git_dir)).unwrap();
        let out = str::from_utf8(&out).unwrap();
        let expected = "cargo:rerun-if-changed=.git/index
cargo:rerun-if-changed=.git/HEAD
cargo:rerun-if-changed=.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]{40}\n";
        assert!(
            Regex::new(expected).unwrap().is_match(out),
            "child __init output did not match: {out}"
        );
        return;
    }

    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();
    init_git_repo(git_dir);

    let output = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("--nocapture")
        .arg("test::test_ambient_git_env_vars_are_ignored")
        .env(CHILD_REPO_ENV, git_dir)
        .env("GIT_DIR", "/nonexistent")
        .env("GIT_WORK_TREE", "/nonexistent")
        .env("GIT_INDEX_FILE", "/nonexistent")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child process failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn test_published_trailing_whitespace() {
    let tempdir = tempfile::tempdir().unwrap();
    let crate_dir = tempdir.path();

    let vcs_info = r#"{
  "git": {
    "sha1": "0c5255b6f47649305fcb68edccb285510aec71a7 \r\n\t\n"
  },
  "path_in_vcs": ""
}"#;

    let file = crate_dir.join(".cargo_vcs_info.json");
    fs::write(file, vcs_info).unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, crate_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rustc-env=GIT_REVISION=0c5255b6f47649305fcb68edccb285510aec71a7\n";
    println!("{out}");
    println!("{expected}");
    assert_eq!(out, expected);
}
