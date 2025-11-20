pub fn quote_arg_unix(s: &str) -> String {
    if s.is_empty() { return "''".to_string(); }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' { out.push_str("'\"'\"'" ); } else { out.push(ch); }
    }
    out.push('\'');
    out
}

pub fn quote_arg_windows(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("`\""),
            '`' => out.push_str("``"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

pub fn quote_arg(s: &str) -> String {
    if cfg!(windows) { quote_arg_windows(s) } else { quote_arg_unix(s) }
}

pub fn quote_placeholder(placeholder: &str) -> String {
    quote_arg(placeholder)
}

pub fn tokenize_shell_args_unix(s: &str) -> Vec<String> {
    let mut res = Vec::new();
    let mut cur = String::new();
    enum State { None, Single, Double }
    let mut st = State::None;
    let mut esc = false;
    let mut d_paren = 0i32;
    let mut d_brace = 0i32;
    let mut d_bracket = 0i32;
    for ch in s.chars() {
        match st {
            State::None => {
                if esc { cur.push(ch); esc = false; continue; }
                match ch {
                    '\\' => { esc = true; }
                    '\'' => { st = State::Single; }
                    '"' => { st = State::Double; }
                    '(' => { d_paren += 1; cur.push(ch); }
                    ')' => { d_paren = (d_paren - 1).max(0); cur.push(ch); }
                    '{' => { d_brace += 1; cur.push(ch); }
                    '}' => { d_brace = (d_brace - 1).max(0); cur.push(ch); }
                    '[' => { d_bracket += 1; cur.push(ch); }
                    ']' => { d_bracket = (d_bracket - 1).max(0); cur.push(ch); }
                    _ => {
                        if ch.is_whitespace() && d_paren == 0 && d_brace == 0 && d_bracket == 0 {
                            if !cur.is_empty() { res.push(cur.clone()); cur.clear(); }
                        } else {
                            cur.push(ch);
                        }
                    }
                }
            }
            State::Single => {
                if ch == '\'' { st = State::None; } else { cur.push(ch); }
            }
            State::Double => {
                if esc { cur.push(ch); esc = false; }
                else if ch == '\\' { esc = true; }
                else if ch == '"' { st = State::None; }
                else { cur.push(ch); }
            }
        }
    }
    if !cur.is_empty() { res.push(cur); }
    res
}

pub fn tokenize_shell_args_windows(s: &str) -> Vec<String> {
    let mut res = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut esc = false;
    let mut d_paren = 0i32;
    let mut d_brace = 0i32;
    let mut d_bracket = 0i32;
    for ch in s.chars() {
        if in_q {
            if esc { cur.push(ch); esc = false; }
            else if ch == '`' { esc = true; }
            else if ch == '"' { in_q = false; }
            else { cur.push(ch); }
        } else {
            match ch {
                '"' => { in_q = true; }
                '(' => { d_paren += 1; cur.push(ch); }
                ')' => { d_paren = (d_paren - 1).max(0); cur.push(ch); }
                '{' => { d_brace += 1; cur.push(ch); }
                '}' => { d_brace = (d_brace - 1).max(0); cur.push(ch); }
                '[' => { d_bracket += 1; cur.push(ch); }
                ']' => { d_bracket = (d_bracket - 1).max(0); cur.push(ch); }
                _ => {
                    if ch.is_whitespace() && d_paren == 0 && d_brace == 0 && d_bracket == 0 {
                        if !cur.is_empty() { res.push(cur.clone()); cur.clear(); }
                    } else { cur.push(ch); }
                }
            }
        }
    }
    if !cur.is_empty() { res.push(cur); }
    res
}

pub fn tokenize_shell_args(s: &str) -> Vec<String> {
    if cfg!(windows) { tokenize_shell_args_windows(s) } else { tokenize_shell_args_unix(s) }
}