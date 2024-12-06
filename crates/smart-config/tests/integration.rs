//! Integration tests for `smart-config`.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/derives/*.rs");
    t.compile_fail("tests/ui/config/*.rs");
}
