use ripgrep_all::shell::{tokenize_shell_args_unix, tokenize_shell_args_windows, tokenize_shell_args};

#[test]
fn unix_tokenize_quotes() {
    if cfg!(windows) { return; }
    let s = "--a  'b c' \"d e\" f\\ g (echo a b) --color='fg:#112233 bg:#445566'";
    let v = tokenize_shell_args_unix(s);
    assert_eq!(v, vec!["--a", "b c", "d e", "f g", "(echo a b)", "--color=fg:#112233 bg:#445566"]);
}

#[test]
fn windows_tokenize_quotes() {
    if !cfg!(windows) { return; }
    let s = "--a  \"b c\" d e \"f`\"g\" (echo a b) --color=\"fg:#112233 bg:#445566\"";
    let v = tokenize_shell_args_windows(s);
    assert_eq!(v, vec!["--a", "b c", "d", "e", "f\"g", "(echo a b)", "--color=fg:#112233 bg:#445566"]);
}

#[test]
fn generic_tokenize_shell_args() {
    let s = "--one \"two three\" four";
    let v = tokenize_shell_args(s);
    assert!(v.len() >= 3);
}

#[test]
fn preserve_nested_constructs_in_bind() {
    let s = "--bind=ctrl-t:execute-silent(echo \"{q}\" && type \"{}\")+abort";
    let v = tokenize_shell_args(s);
    assert_eq!(v, vec![s]);
}