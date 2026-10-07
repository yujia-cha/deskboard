//! JSONC(주석·트레일링 콤마를 허용하는 JSON) → 순수 JSON 변환.
//!
//! 주석은 **지우지 않고 공백으로 바꾼다** — 개행과 바이트 길이를 그대로 두어야
//! `serde_json` 이 알려주는 line:col 이 사용자가 보는 원본과 맞는다.

/// `//` · `/* */` 주석과 `}`/`]` 앞의 트레일링 콤마를 제거한다. 문자열 안은 건드리지 않는다.
pub fn strip(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut in_str = false;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
                i += 1;
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                // 줄 끝까지 공백으로
                while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                    blank(&mut out, chars[i]);
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                blank(&mut out, '/');
                blank(&mut out, '*');
                i += 2;
                while i < chars.len() {
                    if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        blank(&mut out, '*');
                        blank(&mut out, '/');
                        i += 2;
                        break;
                    }
                    blank(&mut out, chars[i]);
                    i += 1;
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    strip_trailing_commas(&out)
}

/// 개행은 남기고 나머지는 같은 바이트 수의 공백으로.
fn blank(out: &mut String, c: char) {
    if c == '\n' || c == '\r' {
        out.push(c);
    } else {
        for _ in 0..c.len_utf8() {
            out.push(' ');
        }
    }
}

fn strip_trailing_commas(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut in_str = false;
    for (i, &c) in chars.iter().enumerate() {
        if in_str {
            out.push(c);
            // 이스케이프된 문자는 앞 글자가 `\` 인지로 판단 (`\\"` 같은 연속 백슬래시는 짝수 개 세어 본다)
            if c == '"' && !escaped(&chars, i) {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == ',' && next_significant(&chars, i + 1).is_some_and(|n| n == '}' || n == ']') {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// `chars[i]` 앞에 홀수 개의 백슬래시가 있으면 이스케이프된 것이다.
fn escaped(chars: &[char], i: usize) -> bool {
    let mut n = 0;
    let mut j = i;
    while j > 0 && chars[j - 1] == '\\' {
        n += 1;
        j -= 1;
    }
    n % 2 == 1
}

fn next_significant(chars: &[char], from: usize) -> Option<char> {
    chars[from.min(chars.len())..].iter().copied().find(|c| !c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(s: &str) -> Value {
        serde_json::from_str(&strip(s)).expect("valid json")
    }

    #[test]
    fn removes_line_and_block_comments() {
        let v = parse("{ // 줄 주석\n \"a\": 1, /* 블록\n주석 */ \"b\": 2 }");
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], 2);
    }

    #[test]
    fn keeps_comment_markers_inside_strings() {
        let v = parse(r#"{ "url": "http://x.y/*z*/", "s": "a // b" }"#);
        assert_eq!(v["url"], "http://x.y/*z*/");
        assert_eq!(v["s"], "a // b");
    }

    #[test]
    fn handles_escaped_quotes() {
        let v = parse(r#"{ "s": "he said \"// no\"", "t": "back\\", /* c */ "u": 1 }"#);
        assert_eq!(v["s"], "he said \"// no\"");
        assert_eq!(v["t"], "back\\");
        assert_eq!(v["u"], 1);
    }

    #[test]
    fn removes_trailing_commas() {
        let v = parse("{ \"a\": [1, 2, ], \"b\": { \"c\": 1, }, }");
        assert_eq!(v["a"].as_array().unwrap().len(), 2);
        assert_eq!(v["b"]["c"], 1);
    }

    #[test]
    fn trailing_comma_inside_string_untouched() {
        let v = parse(r#"{ "s": "a,]" }"#);
        assert_eq!(v["s"], "a,]");
    }

    #[test]
    fn preserves_line_numbers_and_columns() {
        let src = "{\n  // 주석\n  /* a\n b */\n  \"a\": 1,\n  \"b\": oops\n}";
        let stripped = strip(src);
        assert_eq!(stripped.lines().count(), src.lines().count());
        assert_eq!(stripped.len(), src.len());
        let err = serde_json::from_str::<Value>(&stripped).unwrap_err();
        assert_eq!(err.line(), 6);
    }
}
