use std::path::PathBuf;
use url::Url;

/// Convert a drag/drop payload into the first usable local filesystem path.
pub fn first_path_from_drop_payload(payload: &str) -> Option<PathBuf> {
    payload
        .lines()
        .map(str::trim)
        .find(is_candidate_drop_line)
        .map(path_from_drop_line)
}

fn is_candidate_drop_line(line: &&str) -> bool {
    !line.is_empty() && !line.starts_with('#')
}

fn path_from_drop_line(line: &str) -> PathBuf {
    Url::parse(line)
        .ok()
        .filter(|url| url.scheme() == "file")
        .and_then(|url| url.to_file_path().ok())
        .unwrap_or_else(|| PathBuf::from(strip_wrapping_quotes(line)))
}

fn strip_wrapping_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_path() {
        let path = first_path_from_drop_payload(r#""D:\mods\patch_0""#).expect("path");
        assert_eq!(path, PathBuf::from(r#"D:\mods\patch_0"#));
    }

    #[test]
    fn parses_uri_list() {
        let path =
            first_path_from_drop_payload("# comment\nfile:///D:/Games/Helldivers%202/data\n")
                .expect("path");
        assert_eq!(path, PathBuf::from(r"D:\Games\Helldivers 2\data"));
    }
}
