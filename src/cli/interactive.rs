//! Dialoguer-based interactive fill-in for missing CLI arguments.
//!
//! Strategy: fields the user already provided on the command line win
//! unchanged. Anything still empty after parse is prompted for here. When
//! `--non-interactive` is set we bail with a clear message instead of
//! prompting.

use crate::cli::args::Cli;
use crate::index::ArchiveIndex;
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect, Input, MultiSelect, Select};
use eyre::WrapErr;
use std::path::PathBuf;
use walkdir::WalkDir;

pub fn fill_in(cli: &mut Cli, index: &ArchiveIndex) -> crate::Result<()> {
    let theme = ColorfulTheme::default();

    if cli.patch.is_none() {
        cli.patch = Some(prompt_patch_path(&theme, cli.non_interactive)?);
    }

    if cli.data_dir.is_none() {
        cli.data_dir = Some(prompt_path(
            &theme,
            "Game data directory",
            None,
            cli.non_interactive,
            "--data-dir",
        )?);
    }

    let categories: Vec<&str> = index.categories().collect();
    if !categories.iter().any(|c| *c == cli.category) {
        if cli.non_interactive {
            eyre::bail!(
                "--category {:?} not present in archive index (have: {:?})",
                cli.category,
                categories
            );
        }
        let default_idx = categories.iter().position(|c| *c == "Armor").unwrap_or(0);
        let choice = Select::with_theme(&theme)
            .with_prompt("Armor category")
            .items(&categories)
            .default(default_idx)
            .interact()
            .wrap_err("category prompt")?;
        cli.category = categories[choice].to_string();
    }

    if cli.target.is_empty() {
        let entries = index
            .category(&cli.category)
            .ok_or_else(|| eyre::eyre!("unknown category {:?}", cli.category))?;
        if !cli.non_interactive && !entries.is_empty() {
            let names: Vec<String> = entries
                .iter()
                .map(|a| format!("{}  ({})", a.name, a.hash))
                .collect();
            let target_defaults = selected_by_default(entries);
            let chosen = MultiSelect::with_theme(&theme)
                .with_prompt("Targets (space to toggle, Enter to confirm)")
                .items(&names)
                .defaults(&target_defaults)
                .interact()
                .wrap_err("targets prompt")?;
            cli.target = chosen
                .into_iter()
                .map(|i| entries[i].hash.clone())
                .collect();
        }
    }

    if cli.out_dir.is_none() {
        cli.out_dir = Some(prompt_path(
            &theme,
            "Output directory",
            Some("out"),
            cli.non_interactive,
            "--out-dir",
        )?);
    }

    Ok(())
}

fn prompt_patch_path(theme: &ColorfulTheme, non_interactive: bool) -> crate::Result<PathBuf> {
    if non_interactive {
        eyre::bail!("--patch is required in --non-interactive mode");
    }
    let candidates = discover_patch_files(std::env::current_dir()?);
    if !candidates.is_empty() {
        let labels: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
        let idx = FuzzySelect::with_theme(theme)
            .with_prompt("Select patch file")
            .items(&labels)
            .default(0)
            .interact()
            .wrap_err("patch prompt")?;
        return Ok(candidates[idx].clone());
    }
    let raw: String = Input::with_theme(theme)
        .with_prompt("Path to patch file (e.g. 9ba626afa44a3aa3.patch_0)")
        .interact_text()
        .wrap_err("patch path input")?;
    Ok(interactive_path_from(raw))
}

fn discover_patch_files(root: PathBuf) -> Vec<PathBuf> {
    WalkDir::new(root)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.contains(".patch_")
                && !name.ends_with(".gpu_resources")
                && !name.ends_with(".stream")
            {
                Some(e.path().to_path_buf())
            } else {
                None
            }
        })
        .take(40)
        .collect()
}

fn prompt_path(
    theme: &ColorfulTheme,
    label: &str,
    default: Option<&str>,
    non_interactive: bool,
    flag: &str,
) -> crate::Result<PathBuf> {
    if non_interactive {
        eyre::bail!("{} is required in --non-interactive mode", flag);
    }
    let mut prompt = Input::<String>::with_theme(theme).with_prompt(label);
    if let Some(d) = default {
        prompt = prompt.default(d.into());
    }
    let raw = prompt
        .interact_text()
        .wrap_err_with(|| format!("{label} prompt"))?;
    Ok(interactive_path_from(raw))
}

fn interactive_path_from(raw: String) -> PathBuf {
    PathBuf::from(strip_wrapping_double_quotes(raw.trim()))
}

fn strip_wrapping_double_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
}

fn selected_by_default(entries: &[crate::index::ArmorEntry]) -> Vec<bool> {
    entries
        .iter()
        .map(|entry| {
            !crate::target_exclusions::is_default_excluded_target(&entry.hash, &entry.name)
        })
        .collect()
}

/// Confirm proceeding once the plan has been resolved.
pub fn confirm_run(
    theme: &ColorfulTheme,
    non_interactive: bool,
    summary: &str,
) -> crate::Result<bool> {
    if non_interactive {
        return Ok(true);
    }
    let ok = Confirm::with_theme(theme)
        .with_prompt(format!("{summary}\nProceed?"))
        .default(true)
        .interact()
        .wrap_err("confirm")?;
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_wrapping_double_quotes_from_interactive_paths() {
        let path = interactive_path_from(r#""C:\Games\Helldivers 2\data""#.to_string());
        assert_eq!(path, PathBuf::from(r#"C:\Games\Helldivers 2\data"#));
    }

    #[test]
    fn trims_whitespace_before_stripping_quotes() {
        let path = interactive_path_from("  \"out dir\"  ".to_string());
        assert_eq!(path, PathBuf::from("out dir"));
    }

    #[test]
    fn leaves_unbalanced_quotes_untouched_after_trim() {
        let path = interactive_path_from("\"out dir".to_string());
        assert_eq!(path, PathBuf::from("\"out dir"));
    }

    #[test]
    fn target_prompt_defaults_to_all_selected() {
        let entries = vec![
            crate::index::ArmorEntry {
                hash: "a".to_string(),
                name: "A".to_string(),
                extra: Default::default(),
            },
            crate::index::ArmorEntry {
                hash: "b".to_string(),
                name: "B".to_string(),
                extra: Default::default(),
            },
        ];
        assert_eq!(selected_by_default(&entries), vec![true, true]);
    }

    #[test]
    fn target_prompt_defaults_exclude_sa_7() {
        let entries = vec![
            crate::index::ArmorEntry {
                hash: "d895f447d332c331".to_string(),
                name: "SA-7 Headfirst".to_string(),
                extra: Default::default(),
            },
            crate::index::ArmorEntry {
                hash: "a".to_string(),
                name: "A".to_string(),
                extra: Default::default(),
            },
        ];
        assert_eq!(selected_by_default(&entries), vec![false, true]);
    }
}
