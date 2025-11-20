use ripgrep_all::shell::{quote_arg_unix, quote_arg_windows, quote_arg};
use ripgrep_all::shell::tokenize_shell_args;

#[test]
fn quote_unix_basic() {
    if cfg!(windows) { return; }
    assert_eq!(quote_arg_unix("foo bar"), "'foo bar'");
    assert_eq!(quote_arg_unix("a'b"), "'a'\"'\"'b'");
    assert_eq!(quote_arg_unix("$(rm -rf /)"), "'$(rm -rf /)'");
}

#[test]
fn quote_windows_basic() {
    if !cfg!(windows) { return; }
    assert_eq!(quote_arg_windows("foo bar"), "\"foo bar\"");
    assert_eq!(quote_arg_windows("a\"b"), "\"a`\"b\"");
    assert_eq!(quote_arg_windows("tick`here"), "\"tick``here\"");
}

#[test]
fn cross_platform_quote_arg() {
    let s = "space and 'quote' and \"dquote\" and $(sub)";
    let q = quote_arg(s);
    assert!(q.starts_with("'") || q.starts_with("\""));
    assert!(q.ends_with("'") || q.ends_with("\""));
}

#[test]
fn assemble_preview_string_contains_safe_quotes() {
    let preproc = if cfg!(windows) { "C:/Program Files/rga/rga.exe" } else { "/usr/local/bin/rga" };
    let preview = format!(
        "--preview={} --pretty --context 5 -- {} --rga-fzf-path=_{}",
        quote_arg(preproc), quote_arg("{q}"), quote_arg("{}")
    );
    assert!(preview.contains("--pretty"));
    assert!(preview.contains("--context 5"));
    assert!(preview.contains("--rga-fzf-path=_"));
    assert!(preview.contains("{q}") && preview.contains("{}"));
    // Ensure placeholders are wrapped in quotes
    if cfg!(windows) {
        assert!(preview.contains("\"{q}\""));
        assert!(preview.contains("_\"{}\""));
    } else {
        assert!(preview.contains("'{q}'"));
        assert!(preview.contains("_'{}'"));
    }
}

#[test]
fn bind_enter_uses_multi_placeholder() {
    let open = if cfg!(windows) { "C:/Program Files/rga/rga-fzf-open.exe" } else { "/usr/local/bin/rga-fzf-open" };
    let q = quote_arg("{q}");
    let single = format!("--bind=ctrl-m:execute:{} {} {}", quote_arg(open), q, quote_arg("{}"));
    assert!(single.contains("{}"));
    let multi = format!("--bind=ctrl-m:execute:{} {} {}", quote_arg(open), q, quote_arg("{+}"));
    assert!(multi.contains("{+}"));
}

#[test]
fn preview_params_are_quoted_tokens() {
    let params = "--smart-case --color=\"fg:#112233 bg:#445566\"";
    let toks = tokenize_shell_args(params);
    assert_eq!(toks, vec!["--smart-case", "--color=fg:#112233 bg:#445566"]);
}

#[test]
fn default_command_honors_initial_dir() {
    let rg = quote_arg(if cfg!(windows) { "C:/bin/rga.exe" } else { "/usr/bin/rga" });
    let q = quote_arg("hello");
    let dir = quote_arg(if cfg!(windows) { "C:/Users/me/Documents" } else { "/home/me/docs" });
    let cmd = format!("{} {} {}", format!("{} --files-with-matches --rga-cache-max-blob-len=10M", rg), q, dir);
    assert!(cmd.ends_with(&format!(" {}", dir)));
}

#[test]
fn preview_window_spec_is_injected() {
    let spec = "60%:wrap";
    let arg = format!("--preview-window={}", spec);
    assert_eq!(arg, "--preview-window=60%:wrap");
}