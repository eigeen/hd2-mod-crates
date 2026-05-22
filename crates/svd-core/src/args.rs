use crate::error::{Result, message};
use crate::export::{ExportMode, ExportOptions};
use crate::pack::PackOptions;
use std::path::PathBuf;

/// Parses the packer CLI flags.
pub fn parse_pack_args(args: impl IntoIterator<Item = String>) -> Result<PackOptions> {
    let mut parser = ArgParser::new(args);
    let input = parser.required_path("--input")?;
    let base = parser.required_string("--base")?;
    let output = parser.required_path("--output")?;
    let zip_output = parser
        .optional_path("--zip")?
        .or_else(|| Some(default_zip_path(&output)));
    let level = parser.optional_i32("--level")?.unwrap_or(3);
    let jobs = parser.optional_usize("--jobs")?;
    if jobs == Some(0) {
        return Err(message("--jobs must be greater than 0"));
    }
    parser.finish()?;

    Ok(PackOptions {
        input,
        base,
        output,
        zip_output,
        level,
        jobs,
    })
}

/// Parses the exporter CLI flags.
pub fn parse_export_args(args: impl IntoIterator<Item = String>) -> Result<ExportOptions> {
    let mut parser = ArgParser::new(args);
    let package = parser.required_path("--package")?;
    let output = parser.optional_path("--output")?;
    let list = parser.take_flag("--list");
    let all = parser.take_flag("--all");
    let variants = parser.optional_strings("--variant")?;
    let jobs = parser.optional_usize("--jobs")?;
    if jobs == Some(0) {
        return Err(message("--jobs must be greater than 0"));
    }
    parser.finish()?;

    let mode = match (list, all, variants.is_empty()) {
        (true, false, true) => ExportMode::List,
        (true, _, _) => return Err(message("use --list by itself")),
        (false, true, true) => ExportMode::All,
        (false, false, true) => ExportMode::All,
        (false, false, false) => ExportMode::Variants(variants),
        (false, true, false) => return Err(message("use either --all or --variant, not both")),
    };

    Ok(ExportOptions {
        package,
        output,
        mode,
        jobs,
    })
}

fn default_zip_path(output: &PathBuf) -> PathBuf {
    output.with_extension("zip")
}

struct ArgParser {
    args: Vec<String>,
}

impl ArgParser {
    fn new(args: impl IntoIterator<Item = String>) -> Self {
        Self {
            args: args.into_iter().skip(1).collect(),
        }
    }

    fn required_path(&mut self, key: &str) -> Result<PathBuf> {
        self.required_string(key).map(PathBuf::from)
    }

    fn required_string(&mut self, key: &str) -> Result<String> {
        self.optional_string(key)?
            .ok_or_else(|| message(format!("missing required flag {key}")))
    }

    fn optional_path(&mut self, key: &str) -> Result<Option<PathBuf>> {
        Ok(self.optional_string(key)?.map(PathBuf::from))
    }

    fn optional_i32(&mut self, key: &str) -> Result<Option<i32>> {
        self.optional_string(key)?
            .map(|value| value.parse::<i32>().map_err(Into::into))
            .transpose()
    }

    fn optional_usize(&mut self, key: &str) -> Result<Option<usize>> {
        self.optional_string(key)?
            .map(|value| value.parse::<usize>().map_err(Into::into))
            .transpose()
    }

    fn optional_string(&mut self, key: &str) -> Result<Option<String>> {
        let Some(index) = self.args.iter().position(|arg| arg == key) else {
            return Ok(None);
        };
        self.args.remove(index);
        if index >= self.args.len() {
            return Err(message(format!("missing value for {key}")));
        }
        Ok(Some(self.args.remove(index)))
    }

    fn optional_strings(&mut self, key: &str) -> Result<Vec<String>> {
        let mut values = Vec::new();
        while let Some(value) = self.optional_string(key)? {
            values.push(value);
        }
        Ok(values)
    }

    fn take_flag(&mut self, key: &str) -> bool {
        let Some(index) = self.args.iter().position(|arg| arg == key) else {
            return false;
        };
        self.args.remove(index);
        true
    }

    fn finish(self) -> Result<()> {
        if self.args.is_empty() {
            return Ok(());
        }
        Err(message(format!(
            "unknown arguments: {}",
            self.args.join(" ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_export_args, parse_pack_args};
    use crate::export::ExportMode;
    use std::path::PathBuf;

    #[test]
    fn pack_defaults_zip_next_to_output() {
        let options = parse_pack_args([
            "svd-pack".to_owned(),
            "--input".to_owned(),
            "source".to_owned(),
            "--base".to_owned(),
            "Base".to_owned(),
            "--output".to_owned(),
            "dist".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.zip_output, Some(PathBuf::from("dist.zip")));
    }

    #[test]
    fn export_defaults_to_all_without_selection() {
        let options = parse_export_args([
            "svd-export".to_owned(),
            "--package".to_owned(),
            "dist.zip".to_owned(),
            "--output".to_owned(),
            "out".to_owned(),
        ])
        .unwrap();

        assert!(matches!(options.mode, ExportMode::All));
    }

    #[test]
    fn export_accepts_repeated_variants_and_list() {
        let selected = parse_export_args([
            "svd-export".to_owned(),
            "--package".to_owned(),
            "dist.zip".to_owned(),
            "--variant".to_owned(),
            "A".to_owned(),
            "--variant".to_owned(),
            "B".to_owned(),
            "--output".to_owned(),
            "out".to_owned(),
        ])
        .unwrap();
        assert!(matches!(selected.mode, ExportMode::Variants(names) if names == ["A", "B"]));

        let list = parse_export_args([
            "svd-export".to_owned(),
            "--package".to_owned(),
            "dist.zip".to_owned(),
            "--list".to_owned(),
        ])
        .unwrap();
        assert!(matches!(list.mode, ExportMode::List));
    }
}
