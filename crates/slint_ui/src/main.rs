mod app;
mod migration;
mod path_import;

slint::include_modules!();

fn main() -> eyre::Result<()> {
    app::run()
}
