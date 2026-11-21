pub fn sanitize_file_stem(name: &str) -> Option<String> {
    let out: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '\0' => '_',
            '|' => '｜',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    if out.is_empty() || out == "." || out == ".." {
        None
    } else {
        Some(out)
    }
}
