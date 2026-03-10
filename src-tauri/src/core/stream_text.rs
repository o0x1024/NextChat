pub fn merge_stream_text(buffer: &mut String, chunk: &str) -> Option<String> {
    if chunk.is_empty() {
        return None;
    }

    if chunk.starts_with(buffer.as_str()) {
        if chunk.len() == buffer.len() {
            return None;
        }

        let delta = chunk[buffer.len()..].to_string();
        buffer.clear();
        buffer.push_str(chunk);
        return Some(delta);
    }

    let overlap = longest_suffix_prefix_overlap(buffer, chunk);
    if overlap >= 2 {
        let delta = slice_from_char(chunk, overlap).to_string();
        if delta.is_empty() {
            return None;
        }
        buffer.push_str(&delta);
        return Some(delta);
    }

    buffer.push_str(chunk);
    Some(chunk.to_string())
}

fn longest_suffix_prefix_overlap(buffer: &str, chunk: &str) -> usize {
    let buffer_chars = buffer.chars().collect::<Vec<_>>();
    let chunk_chars = chunk.chars().collect::<Vec<_>>();
    let max_overlap = buffer_chars.len().min(chunk_chars.len());

    for overlap in (1..=max_overlap).rev() {
        if buffer_chars[buffer_chars.len() - overlap..] == chunk_chars[..overlap] {
            return overlap;
        }
    }

    0
}

fn slice_from_char(content: &str, char_index: usize) -> &str {
    if char_index == 0 {
        return content;
    }

    match content.char_indices().nth(char_index) {
        Some((byte_index, _)) => &content[byte_index..],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::merge_stream_text;

    #[test]
    fn appends_plain_deltas() {
        let mut buffer = String::new();

        assert_eq!(
            merge_stream_text(&mut buffer, "hello"),
            Some("hello".into())
        );
        assert_eq!(buffer, "hello");

        assert_eq!(
            merge_stream_text(&mut buffer, " world"),
            Some(" world".into())
        );
        assert_eq!(buffer, "hello world");
    }

    #[test]
    fn trims_prefix_snapshots_to_new_suffix() {
        let mut buffer = String::new();

        assert_eq!(merge_stream_text(&mut buffer, "我将"), Some("我将".into()));
        assert_eq!(buffer, "我将");

        assert_eq!(
            merge_stream_text(&mut buffer, "我将针对当前项目"),
            Some("针对当前项目".into())
        );
        assert_eq!(buffer, "我将针对当前项目");
    }

    #[test]
    fn ignores_identical_snapshots() {
        let mut buffer = String::from("same");

        assert_eq!(merge_stream_text(&mut buffer, "same"), None);
        assert_eq!(buffer, "same");
    }

    #[test]
    fn trims_overlapping_window_chunks() {
        let mut buffer = String::from("我将对本");

        assert_eq!(
            merge_stream_text(&mut buffer, "对本项目"),
            Some("项目".into())
        );
        assert_eq!(buffer, "我将对本项目");
    }

    #[test]
    fn ignores_single_char_snapshots_and_appends_next_delta() {
        let mut buffer = String::from("a");

        assert_eq!(merge_stream_text(&mut buffer, "a"), None);
        assert_eq!(buffer, "a");

        assert_eq!(merge_stream_text(&mut buffer, "b"), Some("b".into()));
        assert_eq!(buffer, "ab");
    }
}
