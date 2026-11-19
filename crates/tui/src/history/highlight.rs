use ratatui::style::Style;

use crate::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct HighlightState {
    pub(super) in_block_comment: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lang {
    Rust,
    JsTs,
    Python,
    Go,
    Shell,
    Json,
    Config,
    Sql,
    CFamily,
    Diff,
    Plain,
}

impl Lang {
    fn from_info(info: &str) -> Self {
        let token = info
            .trim()
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match token.as_str() {
            "rust" | "rs" => Self::Rust,
            "ts" | "tsx" | "typescript" | "js" | "jsx" | "mjs" | "cjs" | "javascript" => Self::JsTs,
            "py" | "python" => Self::Python,
            "go" | "golang" => Self::Go,
            "sh" | "bash" | "zsh" | "shell" | "fish" => Self::Shell,
            "json" | "jsonc" => Self::Json,
            "toml" | "yaml" | "yml" => Self::Config,
            "sql" => Self::Sql,
            "c" | "cpp" | "c++" | "cc" | "h" | "hpp" | "java" | "kt" | "swift" => Self::CFamily,
            "diff" | "patch" => Self::Diff,
            _ => Self::Plain,
        }
    }
}

pub(super) fn highlight_line(
    info: &str,
    line: &str,
    state: &mut HighlightState,
    theme: Theme,
) -> Vec<Style> {
    let char_count = line.chars().count();
    let mut styles = vec![Style::default(); char_count];
    if char_count == 0 || theme.is_dumb {
        return styles;
    }
    let lang = Lang::from_info(info);
    if lang == Lang::Plain {
        return styles;
    }
    if lang == Lang::Diff {
        let style = if line.starts_with('+') {
            theme.diff_add_style()
        } else if line.starts_with('-') {
            theme.diff_remove_style()
        } else if line.starts_with("@@") {
            theme.accent_style()
        } else {
            Style::default()
        };
        styles.fill(style);
        return styles;
    }

    let chars: Vec<char> = line.chars().collect();
    let keyword_style = Style::default().fg(theme.syntax_keyword);
    let type_style = Style::default().fg(theme.syntax_type);
    let string_style = Style::default().fg(theme.syntax_string);
    let number_style = Style::default().fg(theme.syntax_number);
    let comment_style = Style::default().fg(theme.syntax_comment);

    let mut i = 0;
    while i < chars.len() {
        if state.in_block_comment {
            let start = i;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    i += 2;
                    state.in_block_comment = false;
                    break;
                }
                i += 1;
            }
            styles[start..i].fill(comment_style);
            continue;
        }

        // Line & block comments
        if has_slash_comments(lang) && chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            styles[i..].fill(comment_style);
            break;
        }
        if has_slash_comments(lang) && chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            let start = i;
            i += 2;
            state.in_block_comment = true;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    i += 2;
                    state.in_block_comment = false;
                    break;
                }
                i += 1;
            }
            styles[start..i].fill(comment_style);
            continue;
        }
        if has_hash_comments(lang) && chars[i] == '#' {
            styles[i..].fill(comment_style);
            break;
        }
        if lang == Lang::Sql && chars[i] == '-' && chars.get(i + 1) == Some(&'-') {
            styles[i..].fill(comment_style);
            break;
        }

        // Strings ("...", '...', backticks in JS/TS/Go/Shell)
        let quote = chars[i];
        let is_single_quote_string = quote == '\'' && lang != Lang::Rust;
        let is_backtick_string =
            quote == '`' && matches!(lang, Lang::JsTs | Lang::Go | Lang::Shell);
        if quote == '"' || is_single_quote_string || is_backtick_string {
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < chars.len() {
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                if chars[i] == '\\' {
                    escaped = true;
                    i += 1;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            styles[start..i].fill(string_style);
            continue;
        }

        // Rust lifetimes vs char literals
        if quote == '\''
            && lang == Lang::Rust
            && let Some(end) = scan_rust_char_literal(&chars, i)
        {
            styles[i..end].fill(string_style);
            i = end;
            continue;
        }

        // Numeric literals
        if chars[i].is_ascii_digit() && (i == 0 || !is_ident_continue(chars[i - 1])) {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '.'))
            {
                i += 1;
            }
            styles[start..i].fill(number_style);
            continue;
        }

        // Identifiers / keywords
        if is_ident_start(chars[i]) {
            let start = i;
            while i < chars.len() && is_ident_continue(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if is_keyword(lang, &word) {
                styles[start..i].fill(keyword_style);
            } else if is_literal_constant(lang, &word) {
                styles[start..i].fill(number_style);
            } else if is_type_name(lang, &word) {
                styles[start..i].fill(type_style);
            }
            continue;
        }

        i += 1;
    }

    styles
}

fn has_slash_comments(lang: Lang) -> bool {
    matches!(
        lang,
        Lang::Rust | Lang::JsTs | Lang::Go | Lang::Json | Lang::Sql | Lang::CFamily
    )
}

fn has_hash_comments(lang: Lang) -> bool {
    matches!(lang, Lang::Python | Lang::Shell | Lang::Config)
}

fn scan_rust_char_literal(chars: &[char], start: usize) -> Option<usize> {
    let second = *chars.get(start + 1)?;
    if second == '\\' {
        let mut idx = start + 2;
        while idx < chars.len() && idx <= start + 8 {
            if chars[idx] == '\'' {
                return Some(idx + 1);
            }
            idx += 1;
        }
        None
    } else if chars.get(start + 2) == Some(&'\'') && second != '\'' {
        Some(start + 3)
    } else {
        None
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_keyword(lang: Lang, word: &str) -> bool {
    match lang {
        Lang::Rust => matches!(
            word,
            "as" | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
        ),
        Lang::JsTs => matches!(
            word,
            "as" | "async"
                | "await"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "else"
                | "enum"
                | "export"
                | "extends"
                | "finally"
                | "for"
                | "from"
                | "function"
                | "if"
                | "implements"
                | "import"
                | "in"
                | "interface"
                | "let"
                | "new"
                | "of"
                | "return"
                | "switch"
                | "throw"
                | "try"
                | "type"
                | "typeof"
                | "var"
                | "while"
        ),
        Lang::Python => matches!(
            word,
            "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        ),
        Lang::Go => matches!(
            word,
            "break"
                | "case"
                | "chan"
                | "const"
                | "continue"
                | "default"
                | "defer"
                | "else"
                | "fallthrough"
                | "for"
                | "func"
                | "go"
                | "if"
                | "import"
                | "interface"
                | "map"
                | "package"
                | "range"
                | "return"
                | "select"
                | "struct"
                | "switch"
                | "type"
                | "var"
        ),
        Lang::Shell => matches!(
            word,
            "case"
                | "do"
                | "done"
                | "elif"
                | "else"
                | "esac"
                | "export"
                | "fi"
                | "for"
                | "function"
                | "if"
                | "in"
                | "local"
                | "return"
                | "then"
                | "until"
                | "while"
        ),
        Lang::Sql => {
            let upper = word.to_ascii_uppercase();
            matches!(
                upper.as_str(),
                "SELECT"
                    | "FROM"
                    | "WHERE"
                    | "JOIN"
                    | "LEFT"
                    | "RIGHT"
                    | "INNER"
                    | "ON"
                    | "AS"
                    | "AND"
                    | "OR"
                    | "NOT"
                    | "IN"
                    | "INSERT"
                    | "INTO"
                    | "VALUES"
                    | "UPDATE"
                    | "SET"
                    | "DELETE"
                    | "CREATE"
                    | "TABLE"
                    | "DROP"
                    | "ORDER"
                    | "BY"
                    | "GROUP"
                    | "HAVING"
                    | "LIMIT"
                    | "ASC"
                    | "DESC"
            )
        }
        Lang::CFamily => matches!(
            word,
            "break"
                | "case"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "else"
                | "enum"
                | "for"
                | "func"
                | "if"
                | "import"
                | "let"
                | "namespace"
                | "private"
                | "public"
                | "return"
                | "static"
                | "struct"
                | "switch"
                | "template"
                | "typedef"
                | "using"
                | "var"
                | "void"
                | "while"
        ),
        Lang::Json | Lang::Config | Lang::Diff | Lang::Plain => false,
    }
}

fn is_literal_constant(lang: Lang, word: &str) -> bool {
    match lang {
        Lang::Python => matches!(word, "True" | "False" | "None"),
        Lang::Sql => matches!(
            word.to_ascii_uppercase().as_str(),
            "NULL" | "TRUE" | "FALSE"
        ),
        _ => matches!(
            word,
            "true" | "false" | "null" | "nil" | "None" | "Some" | "Ok" | "Err"
        ),
    }
}

fn is_type_name(lang: Lang, word: &str) -> bool {
    match lang {
        Lang::Rust => {
            word.starts_with(|c: char| c.is_ascii_uppercase())
                || matches!(
                    word,
                    "bool"
                        | "char"
                        | "f32"
                        | "f64"
                        | "i8"
                        | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "isize"
                        | "str"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "usize"
                )
        }
        Lang::JsTs | Lang::Go | Lang::Python | Lang::CFamily => {
            word.starts_with(|c: char| c.is_ascii_uppercase())
                || matches!(
                    word,
                    "string" | "number" | "boolean" | "int" | "float" | "str"
                )
        }
        _ => false,
    }
}
