pub mod curl;
pub mod ffmpeg;

use unicode_width::UnicodeWidthStr;

pub fn align_left(s: &str, width: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    let padding = width.saturating_sub(w);
    format!("{}{:<padding$}", s, "", padding = padding)
}

pub fn padding_left(s: &str, count: usize) -> String {
    let mut padding = String::with_capacity(count + 1);
    padding.push('\n');
    padding.extend(std::iter::repeat_n(" ", count));
    s.replace('\n', &padding)
}
