use anyhow::Result;

/// Checks the GitHub repository for a newer release and updates the current executable if found.
pub fn update_binary() -> Result<String> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("woochaq")
        .repo_name("tuneli-tui")
        .bin_name("tuneli-tui")
        .show_download_progress(false)
        .current_version(env!("CARGO_PKG_VERSION"))
        .build()?
        .update()?;
        
    if status.updated() {
        Ok(format!("Successfully updated to v{}! Please restart the app.", status.version()))
    } else {
        Ok(format!("Already at the latest version (v{}).", status.version()))
    }
}
