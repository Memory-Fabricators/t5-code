//! Port of `src/utils/browser.ts` — open URL in the system browser.

use std::process::Command;

fn validate_url(url: &str) -> Result<(), String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(format!(
            "Invalid URL protocol: must use http:// or https://, got {url}"
        ))
    }
}

/// Open a URL with the platform default browser. Returns whether the command succeeded.
pub fn open_browser(url: &str) -> bool {
    if validate_url(url).is_err() {
        return false;
    }

    let browser_env = std::env::var("BROWSER").ok();
    let result = if cfg!(target_os = "windows") {
        if let Some(browser) = browser_env {
            Command::new(browser).arg(url).status()
        } else {
            Command::new("rundll32").args(["url,OpenURL", url]).status()
        }
    } else {
        let command = browser_env.unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "open".into()
            } else {
                "xdg-open".into()
            }
        });
        Command::new(command).arg(url).status()
    };

    matches!(result, Ok(status) if status.success())
}
