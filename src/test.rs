#![cfg(test)]

use regex::Regex;
use std::str;

#[test]
fn test_init() {
    let mut out = Vec::new();
    let res = super::__init(&mut out);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    println!("{out}");
    assert!(
        Regex::new("cargo:rustc-env=GIT_REVISION=[0-9a-f]+(-dirty)?")
            .unwrap()
            .is_match(out)
    );
}
