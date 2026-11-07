//! Syntax highlighting that mimics the output of Rouge, the highlighter used by
//! Jekyll. Each lexer is a small hand-written port of the corresponding Rouge
//! lexer, emitting the same short Pygments-style CSS classes (`k`, `nf`, `mi`,
//! ...), so the minima stylesheet keeps working unchanged.
//!
//! To support a new language, write a lexer function and add it to [`lexer_for`].

use regex::Regex;
use std::sync::LazyLock;

/// A token class (e.g. `"k"` for keyword), or `None` for unstyled text.
type Class = Option<&'static str>;

struct Tokens {
    tokens: Vec<(Class, String)>,
}

impl Tokens {
    fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    /// Push a token, merging it with the previous one if they have the same
    /// class, like Rouge does.
    fn push(&mut self, class: Class, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.tokens.last_mut() {
            Some((c, t)) if *c == class => t.push_str(text),
            _ => self.tokens.push((class, text.to_string())),
        }
    }

    fn to_html(&self) -> String {
        let mut out = String::new();
        for (class, text) in &self.tokens {
            match class {
                Some(class) => {
                    out.push_str("<span class=\"");
                    out.push_str(class);
                    out.push_str("\">");
                    out.push_str(&escape_html(text));
                    out.push_str("</span>");
                }
                None => out.push_str(&escape_html(text)),
            }
        }
        out
    }
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

type Lexer = fn(&str) -> Tokens;

fn lexer_for(lang: &str) -> Option<Lexer> {
    Some(match lang {
        "rust" | "rs" => lex_rust,
        "nasm" => lex_nasm,
        "shell" | "sh" | "bash" | "console" => lex_shell,
        "toml" => lex_toml,
        "brainfuck" | "bf" => lex_brainfuck,
        "plaintext" | "text" | "" => lex_plain,
        _ => return None,
    })
}

/// Render a fenced code block the way kramdown + Rouge does. Languages without
/// a lexer are rendered as a plain `<pre><code>`, which is what Rouge does for
/// unknown languages.
pub fn code_block(lang: &str, code: &str) -> String {
    let Some(lexer) = lexer_for(lang) else {
        return format!(
            "<pre><code class=\"language-{}\">{}</code></pre>\n",
            escape_html(lang),
            escape_html(code)
        );
    };
    let lang = if lang.is_empty() { "plaintext" } else { lang };
    format!(
        "<div class=\"language-{} highlighter-rouge\"><div class=\"highlight\"><pre class=\"highlight\"><code>{}</code></pre></div></div>\n",
        lang,
        lexer(code).to_html()
    )
}

/// Try to match `re` (which must start with `^`) at the start of `s`.
fn at<'a>(re: &Regex, s: &'a str) -> Option<&'a str> {
    re.find(s).map(|m| m.as_str())
}

macro_rules! re {
    ($name:ident = $re:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($re).unwrap());
    };
}

fn lex_plain(code: &str) -> Tokens {
    let mut t = Tokens::new();
    t.push(None, code);
    t
}

fn lex_brainfuck(code: &str) -> Tokens {
    let mut t = Tokens::new();
    for c in code.chars() {
        let class = match c {
            '+' | '-' | '.' | ',' => "nf",
            '<' | '>' => "nb",
            '[' | ']' => "p",
            _ => "c1",
        };
        t.push(Some(class), c.encode_utf8(&mut [0; 4]));
    }
    t
}

const RUST_KEYWORDS: &[&str] = &[
    "abstract", "alignof", "as", "async", "await", "become", "box", "break", "const", "continue",
    "crate", "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl",
    "let", "loop", "macro", "match", "mod", "move", "mut", "offsetof", "override", "priv", "proc",
    "pub", "pure", "ref", "return", "Self", "self", "sizeof", "static", "struct", "super", "trait",
    "true", "try", "type", "typeof", "union", "unsafe", "unsized", "use", "virtual", "where",
    "while", "yield",
];

const RUST_BUILTINS: &[&str] = &[
    "Send",
    "Sized",
    "Sync",
    "Unpin",
    "Drop",
    "Fn",
    "FnMut",
    "FnOnce",
    "drop",
    "Box",
    "ToOwned",
    "Clone",
    "PartialEq",
    "PartialOrd",
    "Eq",
    "Ord",
    "AsRef",
    "AsMut",
    "Into",
    "From",
    "Default",
    "Iterator",
    "Extend",
    "IntoIterator",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "Option",
    "Some",
    "None",
    "Ok",
    "Err",
    "String",
    "ToString",
    "Vec",
    "bool",
    "char",
    "str",
    "u8",
    "u16",
    "u32",
    "u64",
    "u128",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "f32",
    "f64",
    "usize",
    "isize",
];

fn lex_rust(code: &str) -> Tokens {
    re!(WS = r"^\s+");
    re!(LINE_COMMENT = r"^//[^\n]*");
    re!(BLOCK_COMMENT = r"^(?s)/\*.*?\*/");
    re!(IDENT = r"^[a-zA-Z_][a-zA-Z0-9_]*");
    re!(MACRO = r"^[a-zA-Z_][a-zA-Z0-9_]*!");
    re!(CHAR = r"^'(\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'");
    re!(LIFETIME = r"^'[a-zA-Z_][a-zA-Z0-9_]*");
    re!(RAW_STRING = r##"^r(#*)""##);
    re!(STRING_ESCAPE = r"^\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|(?s:.))");
    re!(STRING_TEXT = r#"^[^"\\]+"#);
    re!(FLOAT = r"^[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9_]+)?(f32|f64)?");
    re!(INT = r"^(0x[0-9a-fA-F_]+|0b[01_]+|0o[0-7_]+|[0-9][0-9_]*)([iu](8|16|32|64|128|size))?");
    re!(DOT_IDENT = r"^\.\s*[a-zA-Z_][a-zA-Z0-9_]*");
    re!(DOT_NUM = r"^\.\s*[0-9]+");
    re!(OPERATOR = r"^[-+*/%&<>^!@=?]+");
    re!(PUNCTUATION = r"^(::|[{}()\[\],.;:#$|])");

    let mut t = Tokens::new();
    let mut s = code;
    // Whether the next non-whitespace text is followed by `(`.
    let followed_by = |rest: &str, c: char| rest.trim_start().starts_with(c);
    while !s.is_empty() {
        let (class, len): (Class, usize) = if let Some(m) = at(&WS, s) {
            (None, m.len())
        } else if let Some(m) = at(&LINE_COMMENT, s) {
            (Some("c"), m.len())
        } else if let Some(m) = at(&BLOCK_COMMENT, s) {
            (Some("cm"), m.len())
        } else if s.starts_with("#[") || s.starts_with("#![") {
            // Attributes: everything up to the matching `]`, split on whitespace.
            let mut depth = 0;
            let mut end = s.len();
            for (i, c) in s.char_indices() {
                match c {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let attr = &s[..end];
            let mut rest = attr;
            while !rest.is_empty() {
                let ws = rest.len() - rest.trim_start().len();
                if ws > 0 {
                    t.push(None, &rest[..ws]);
                    rest = &rest[ws..];
                }
                let word = rest.find(char::is_whitespace).unwrap_or(rest.len());
                t.push(Some("nd"), &rest[..word]);
                rest = &rest[word..];
            }
            s = &s[end..];
            continue;
        } else if s.starts_with("=>") || s.starts_with("->") {
            (Some("k"), 2)
        } else if let Some(m) = at(&MACRO, s).filter(|m| !s[m.len()..].starts_with('=')) {
            (Some("nd"), m.len())
        } else if let Some(m) = at(&CHAR, s) {
            (Some("sc"), m.len())
        } else if let Some(m) = at(&LIFETIME, s) {
            (Some("nv"), m.len())
        } else if let Some(caps) = RAW_STRING.captures(s) {
            let close = format!("\"{}", &caps[1]);
            let start = caps[0].len();
            let end = s[start..]
                .find(&close)
                .map_or(s.len(), |i| start + i + close.len());
            (Some("s"), end)
        } else if s.starts_with('"') {
            t.push(Some("s"), "\"");
            s = &s[1..];
            while !s.is_empty() {
                if s.starts_with('"') {
                    t.push(Some("s"), "\"");
                    s = &s[1..];
                    break;
                } else if let Some(m) = at(&STRING_ESCAPE, s) {
                    t.push(Some("se"), m);
                    s = &s[m.len()..];
                } else {
                    let m = at(&STRING_TEXT, s).unwrap_or(&s[..1]);
                    t.push(Some("s"), m);
                    s = &s[m.len()..];
                }
            }
            continue;
        } else if let Some(m) = at(&FLOAT, s) {
            (Some("mf"), m.len())
        } else if let Some(m) = at(&INT, s) {
            (Some("mi"), m.len())
        } else if let Some(m) = at(&IDENT, s) {
            let rest = &s[m.len()..];
            let class = if m == "_" {
                "mi"
            } else if RUST_KEYWORDS.contains(&m) {
                "k"
            } else if followed_by(rest, '(') {
                "nf"
            } else if rest.starts_with("::") {
                "nn"
            } else if RUST_BUILTINS.contains(&m) {
                "nb"
            } else {
                "n"
            };
            (Some(class), m.len())
        } else if let Some(m) = at(&DOT_IDENT, s) {
            let class = if followed_by(&s[m.len()..], '(') {
                "nf"
            } else {
                "py"
            };
            (Some(class), m.len())
        } else if let Some(m) = at(&DOT_NUM, s) {
            (Some("na"), m.len())
        } else if s.starts_with("..") {
            let len = if s.starts_with("..=") { 3 } else { 2 };
            (Some("o"), len)
        } else if let Some(m) = at(&PUNCTUATION, s) {
            (Some("p"), m.len())
        } else if let Some(m) = at(&OPERATOR, s) {
            (Some("o"), m.len())
        } else {
            (None, s.chars().next().unwrap().len_utf8())
        };
        t.push(class, &s[..len]);
        s = &s[len..];
    }
    t
}

const NASM_REGISTERS: &[&str] = &[
    "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "eax", "ebx", "ecx", "edx", "esi",
    "edi", "ebp", "esp", "ax", "bx", "cx", "dx", "si", "di", "bp", "sp", "al", "ah", "bl", "bh",
    "cl", "ch", "dl", "dh", "cs", "ds", "es", "fs", "gs", "ss",
];
const NASM_TYPES: &[&str] = &["byte", "word", "dword", "qword", "tword", "oword"];
const NASM_DATA: &[&str] = &[
    "db", "dw", "dd", "dq", "dt", "do", "resb", "resw", "resd", "resq", "rest", "reso", "incbin",
    "times",
];

fn lex_nasm(code: &str) -> Tokens {
    re!(WS = r"^[ \t]+");
    re!(LABEL = r"^[.a-zA-Z_$?@][.\w$?@#~]*:");
    re!(WORD = r"^[.a-zA-Z_$?@][.\w$?@#~]*");
    re!(NUMBER = r"^(0x[0-9a-fA-F]+|[0-9][0-9a-fA-F]*h|[0-9]+)\b");
    re!(STRING = r#"^('[^'\n]*'|"[^"\n]*")"#);

    let mut t = Tokens::new();
    for line in code.split_inclusive('\n') {
        let mut s = line;
        // An instruction (or directive) is expected at the start of the line,
        // after an optional label.
        let mut expect_instruction = true;
        while !s.is_empty() {
            let (class, len): (Class, usize) = if let Some(m) = at(&WS, s) {
                (None, m.len())
            } else if s.starts_with('\n') {
                (None, 1)
            } else if s.starts_with(';') {
                (Some("c1"), s.trim_end_matches('\n').len())
            } else if let Some(m) = at(&LABEL, s).filter(|_| expect_instruction) {
                (Some("nl"), m.len())
            } else if let Some(m) = at(&WORD, s) {
                let lower = m.to_ascii_lowercase();
                let class = if m == "$" {
                    "kc"
                } else if expect_instruction {
                    expect_instruction = false;
                    if NASM_DATA.contains(&lower.as_str()) {
                        "kd"
                    } else {
                        "nf"
                    }
                } else if NASM_REGISTERS.contains(&lower.as_str()) {
                    "nb"
                } else if NASM_TYPES.contains(&lower.as_str()) {
                    "kt"
                } else {
                    "nv"
                };
                (Some(class), m.len())
            } else if let Some(m) = at(&NUMBER, s) {
                (Some("mi"), m.len())
            } else if let Some(m) = at(&STRING, s) {
                (Some("s"), m.len())
            } else if s.starts_with([',', '[', ']']) {
                (Some("p"), 1)
            } else if s.starts_with([':']) {
                (Some("err"), 1)
            } else if s.starts_with(['+', '-', '*', '/', '<', '>', '%', '|', '&', '^', '~']) {
                (Some("o"), 1)
            } else {
                (None, s.chars().next().unwrap().len_utf8())
            };
            t.push(class, &s[..len]);
            s = &s[len..];
        }
    }
    t
}

fn lex_shell(code: &str) -> Tokens {
    re!(COMMENT = r"^#[^\n]*");
    re!(
        KEYWORD = r"^\b(if|fi|else|while|do|done|for|then|return|function|select|continue|until|esac|elif|in)\s*\b"
    );
    re!(
        BUILTIN = r"^\b(alias|bg|bind|break|builtin|caller|cd|command|compgen|complete|declare|dirs|disown|echo|enable|eval|exec|exit|export|false|fc|fg|getopts|hash|help|history|jobs|kill|let|local|logout|mapfile|popd|pushd|pwd|read|readonly|set|shift|shopt|source|suspend|test|time|times|trap|true|type|typeset|ulimit|umask|unalias|unset|wait|cat|tac|nl|od|base32|base64|fmt|pr|fold|head|tail|split|csplit|wc|sum|cksum|md5sum|sha1sum|sha224sum|sha256sum|sha384sum|sha512sum|sort|shuf|uniq|comm|ptx|tsort|cut|paste|join|tr|expand|unexpand|ls|dir|vdir|dircolors|cp|dd|install|mv|rm|shred|link|ln|mkdir|mkfifo|mknod|readlink|rmdir|unlink|chown|chgrp|chmod|touch|df|du|stat|sync|truncate|printf|yes|expr|tee|basename|dirname|pathchk|mktemp|realpath|stty|printenv|tty|id|logname|whoami|groups|users|who|date|arch|nproc|uname|hostname|hostid|uptime|chcon|runcon|chroot|env|nice|nohup|stdbuf|timeout|sleep|factor|numfmt|seq|tar|grep|sudo|awk|sed|gzip|gunzip)\s*\b"
    );
    re!(ASSIGN = r"^(\b\w+)(=)");
    re!(OPTION = r"^-{1,2}[\w-]+");
    re!(VARIABLE = r"^\$#?(\w+|.)");
    re!(SINGLE_QUOTED = r"^'[^']*'");
    re!(DOUBLE_QUOTED = r#"^"([^"\\]|\\.)*""#);
    re!(TEXT = r#"^[^=\s\[\]{}()$"'`\\<;&|*]+"#);
    re!(WS = r"^\s+");

    let mut t = Tokens::new();
    let mut s = code;
    while !s.is_empty() {
        let (class, len): (Class, usize) = if let Some(m) = at(&COMMENT, s) {
            (Some("c"), m.len())
        } else if let Some(m) = at(&KEYWORD, s) {
            (Some("k"), m.len())
        } else if let Some(m) = at(&BUILTIN, s).filter(|m| !s[m.len()..].starts_with(['.', '-'])) {
            (Some("nb"), m.len())
        } else if s.starts_with('.') && s[1..].starts_with(char::is_whitespace) {
            (Some("nb"), 1)
        } else if let Some(caps) = ASSIGN.captures(s) {
            t.push(Some("nv"), &caps[1]);
            t.push(Some("o"), &caps[2]);
            s = &s[caps[0].len()..];
            continue;
        } else if s.starts_with('[') {
            // A `[` in the middle of a word (like `es:[rdi]`) is plain text.
            let after_space = code[..code.len() - s.len()]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace);
            (after_space.then_some("o"), 1)
        } else if s.starts_with(['{', '}', '(', ')', '!', '=', '>']) {
            (Some("o"), 1)
        } else if s.starts_with('*') {
            (Some("k"), 1)
        } else if s.starts_with("&&") || s.starts_with("||") {
            (Some("o"), 2)
        } else if let Some(m) = at(&WS, s) {
            (None, m.len())
        } else if s.starts_with('\\') && s.len() > 1 {
            (Some("se"), 1 + s[1..].chars().next().unwrap().len_utf8())
        } else if let Some(m) = at(&OPTION, s) {
            (Some("nt"), m.len())
        } else if let Some(m) = at(&DOUBLE_QUOTED, s) {
            (Some("s2"), m.len())
        } else if let Some(m) = at(&SINGLE_QUOTED, s) {
            (Some("s1"), m.len())
        } else if s.starts_with([';', '&', '|']) {
            (Some("p"), 1)
        } else if let Some(m) = at(&VARIABLE, s) {
            (Some("nv"), m.len())
        } else if s.starts_with('`') {
            (Some("sb"), 1)
        } else if let Some(m) = at(&TEXT, s) {
            (None, m.len())
        } else {
            (None, s.chars().next().unwrap().len_utf8())
        };
        t.push(class, &s[..len]);
        s = &s[len..];
    }
    t
}

fn lex_toml(code: &str) -> Tokens {
    re!(WS = r"^\s+");
    re!(COMMENT = r"^#[^\n]*");
    re!(CONSTANT = r"^(true|false)\b");
    re!(TABLE = r"^\[[^\s\]]*\]");
    re!(NUMBER = r"^[+-]?[0-9][0-9_]*(\.[0-9_]+)?");
    re!(KEY = r"^([A-Za-z0-9_]+)(\s*)(=)");
    re!(STRING = r#"^("([^"\\]|\\.)*"|'[^']*')"#);
    re!(NAME = r"^[A-Za-z_][A-Za-z0-9_-]*");

    let mut t = Tokens::new();
    let mut s = code;
    while !s.is_empty() {
        let (class, len): (Class, usize) = if let Some(m) = at(&WS, s) {
            (None, m.len())
        } else if let Some(m) = at(&COMMENT, s) {
            (Some("c"), m.len())
        } else if let Some(m) = at(&CONSTANT, s) {
            (Some("kc"), m.len())
        } else if let Some(m) = at(&TABLE, s) {
            (Some("nn"), m.len())
        } else if let Some(m) = at(&NUMBER, s) {
            (Some("mi"), m.len())
        } else if let Some(caps) = KEY.captures(s) {
            t.push(Some("py"), &caps[1]);
            t.push(None, &caps[2]);
            t.push(Some("p"), &caps[3]);
            s = &s[caps[0].len()..];
            continue;
        } else if let Some(m) = at(&STRING, s) {
            (Some("s"), m.len())
        } else if let Some(m) = at(&NAME, s) {
            (Some("nn"), m.len())
        } else if s.starts_with('=') {
            (Some("o"), 1)
        } else if s.starts_with(['{', '}', ',', '[', ']']) {
            (Some("p"), 1)
        } else {
            (None, s.chars().next().unwrap().len_utf8())
        };
        t.push(class, &s[..len]);
        s = &s[len..];
    }
    t
}
