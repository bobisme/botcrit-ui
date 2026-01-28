//! Text utilities

/// Wrap text to a maximum width, preserving line breaks.
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for raw_line in text.lines() {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            if current.is_empty() {
                if word.len() > max_width {
                    let mut chunk = word;
                    while chunk.len() > max_width {
                        lines.push(chunk[..max_width].to_string());
                        chunk = &chunk[max_width..];
                    }
                    if !chunk.is_empty() {
                        current = chunk.to_string();
                    }
                } else {
                    current.push_str(word);
                }
            } else if current.len() + 1 + word.len() <= max_width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = String::new();

                if word.len() > max_width {
                    let mut chunk = word;
                    while chunk.len() > max_width {
                        lines.push(chunk[..max_width].to_string());
                        chunk = &chunk[max_width..];
                    }
                    if !chunk.is_empty() {
                        current = chunk.to_string();
                    }
                } else {
                    current.push_str(word);
                }
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    lines
}
