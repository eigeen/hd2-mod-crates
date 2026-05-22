/// Returns the cross-variant identity used to pair renamed patch resources.
pub fn logical_file_key(relative_path: &str) -> String {
    let (parent, file_name) = split_parent(relative_path);
    match logical_file_name(file_name) {
        Some(name) => format!("{parent}{name}"),
        None => relative_path.to_owned(),
    }
}

fn split_parent(relative_path: &str) -> (&str, &str) {
    match relative_path.rsplit_once('/') {
        Some((parent, file_name)) => (&relative_path[..parent.len() + 1], file_name),
        None => ("", relative_path),
    }
}

fn logical_file_name(file_name: &str) -> Option<String> {
    let marker = ".patch_";
    let marker_start = file_name.find(marker)?;
    if marker_start == 0 {
        return None;
    }

    let digits_start = marker_start + marker.len();
    let digits_len = file_name[digits_start..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    if digits_len == 0 {
        return None;
    }

    let suffix_start = digits_start + digits_len;
    if !file_name[suffix_start..].is_empty() && !file_name[suffix_start..].starts_with('.') {
        return None;
    }

    Some(format!("*{}", &file_name[marker_start..suffix_start]) + &file_name[suffix_start..])
}

#[cfg(test)]
mod tests {
    use super::logical_file_key;

    #[test]
    fn normalizes_patch_resource_name_prefix() {
        assert_eq!(
            logical_file_key("actors/1d9e8acfc3ee3ace.patch_0.gpu_resources"),
            "actors/*.patch_0.gpu_resources"
        );
        assert_eq!(
            logical_file_key("actors/2d25ef67b7d87ea3.patch_0"),
            "actors/*.patch_0"
        );
    }

    #[test]
    fn leaves_non_patch_paths_unchanged() {
        assert_eq!(logical_file_key("actors/file.bin"), "actors/file.bin");
        assert_eq!(logical_file_key("patch_0/file.bin"), "patch_0/file.bin");
        assert_eq!(
            logical_file_key("actors/file.patch_0backup"),
            "actors/file.patch_0backup"
        );
    }
}
