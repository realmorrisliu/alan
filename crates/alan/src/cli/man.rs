//! `alan man [topic]` command backed by `alan_service_manager::manpages`.

use alan_service_manager::manpages;

/// Render either the `/man` index or one requested manual page.
pub fn format_man(topic: Option<&str>) -> anyhow::Result<String> {
    match topic.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(manpages::index_text()),
        Some(name) => {
            if let Some(page) = manpages::lookup(name) {
                Ok(page.content.to_string())
            } else {
                let available = manpages::PAGES
                    .iter()
                    .map(|page| page.path)
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!("No manual entry for `{name}`. Available pages: {available}");
            }
        }
    }
}

/// Execute `alan man [topic]`.
pub fn run(topic: Option<&str>) -> anyhow::Result<()> {
    println!("{}", format_man(topic)?);
    Ok(())
}
