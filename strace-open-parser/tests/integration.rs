use strace_open_parser::{parse_strace_output, Access};

#[test]
fn test_parse_sample_output() {
    let input = include_str!("fixtures/sample_output.txt");
    let result = parse_strace_output(input);
    assert!(!result.is_empty());
    assert!(result.iter().any(|fa| fa.access == Access::ReadOnly));
    assert!(result.iter().any(|fa| fa.access == Access::ReadWrite));
}
