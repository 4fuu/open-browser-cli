use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "match")]
    pub match_pattern: String,
    pub trigger: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub wait: Option<String>,
    pub timeout: Option<u64>,
    pub action: String,
    pub value: Option<String>,
}

fn config_dir_from_env<F>(get_env: F, is_windows: bool) -> Result<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    if is_windows {
        if let Some(appdata) = get_env("APPDATA").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(appdata));
        }

        if let Some(user_profile) = get_env("USERPROFILE").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(user_profile).join("AppData/Roaming"));
        }

        anyhow::bail!("APPDATA or USERPROFILE environment variable not set");
    }

    if let Some(xdg_config_home) = get_env("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(xdg_config_home));
    }

    if let Some(home) = get_env("HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home).join(".config"));
    }

    anyhow::bail!("XDG_CONFIG_HOME or HOME environment variable not set");
}

fn browser_cli_config_dir() -> Result<PathBuf> {
    config_dir_from_env(|key| std::env::var(key).ok(), cfg!(windows))
}

fn plugins_dir() -> Result<PathBuf> {
    Ok(browser_cli_config_dir()?.join("browser-cli/plugins"))
}

/// Load a plugin by name from the browser-cli config directory.
pub fn load_plugin(name: &str) -> Result<Plugin> {
    let path = plugins_dir()?.join(format!("{name}.toml"));
    if !path.exists() {
        anyhow::bail!("plugin '{name}' not found at {}", path.display());
    }
    load_plugin_from_path(&path)
}

/// Load a plugin from an explicit file path.
pub fn load_plugin_from_path(path: &Path) -> Result<Plugin> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read plugin file: {}", path.display()))?;
    let plugin: Plugin =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(plugin)
}

/// List all available plugins from the browser-cli config directory.
pub fn list_plugins() -> Result<Vec<Plugin>> {
    let dir = plugins_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut plugins = Vec::new();
    let entries = std::fs::read_dir(&dir)
        .with_context(|| format!("failed to read plugins directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            match load_plugin_from_path(&path) {
                Ok(plugin) => plugins.push(plugin),
                Err(e) => {
                    eprintln!("warning: skipping {}: {e}", path.display());
                }
            }
        }
    }

    Ok(plugins)
}

/// Convert a glob pattern (with `*` matching non-`/` chars) to a regex.
fn glob_to_regex(pattern: &str) -> Result<Regex> {
    let mut regex = String::from("^");
    for c in pattern.chars() {
        match c {
            '*' => regex.push_str("[^/]*"),
            '?' => regex.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
    }
    regex.push('$');
    Regex::new(&regex).context("failed to compile glob pattern as regex")
}

/// Load all plugins and return those whose match pattern matches the given URL.
pub fn find_matching_plugins(url: &str) -> Result<Vec<Plugin>> {
    let all = list_plugins()?;
    let mut matched = Vec::new();
    for plugin in all {
        match glob_to_regex(&plugin.match_pattern) {
            Ok(re) => {
                if re.is_match(url) {
                    matched.push(plugin);
                }
            }
            Err(e) => {
                eprintln!(
                    "warning: invalid match pattern '{}' in plugin '{}': {e}",
                    plugin.match_pattern, plugin.name
                );
            }
        }
    }
    Ok(matched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_toml() -> &'static str {
        r#"
name = "skip-cookie-banner"
description = "Auto close cookie consent popup"
match = "*.example.com/*"
trigger = "on_load"

[[steps]]
wait = "button:contains('Accept')"
timeout = 3000
action = "click"

[[steps]]
wait = "500"
action = "wait"
"#
    }

    fn sample_toml_no_description() -> &'static str {
        r#"
name = "simple-plugin"
match = "*.example.com/*"
trigger = "on_load"

[[steps]]
action = "wait"
wait = "1000"
"#
    }

    #[test]
    fn test_config_dir_from_env_windows_prefers_appdata() {
        let dir = config_dir_from_env(
            |key| match key {
                "APPDATA" => Some(r"C:\Users\alice\AppData\Roaming".to_string()),
                "USERPROFILE" => Some(r"C:\Users\alice".to_string()),
                _ => None,
            },
            true,
        )
        .unwrap();

        assert_eq!(dir, PathBuf::from(r"C:\Users\alice\AppData\Roaming"));
    }

    #[test]
    fn test_config_dir_from_env_windows_falls_back_to_userprofile() {
        let dir = config_dir_from_env(
            |key| match key {
                "USERPROFILE" => Some(r"C:\Users\alice".to_string()),
                _ => None,
            },
            true,
        )
        .unwrap();

        assert_eq!(
            dir,
            PathBuf::from(r"C:\Users\alice")
                .join("AppData")
                .join("Roaming")
        );
    }

    #[test]
    fn test_config_dir_from_env_unix_prefers_xdg() {
        let dir = config_dir_from_env(
            |key| match key {
                "XDG_CONFIG_HOME" => Some("/tmp/xdg-config".to_string()),
                "HOME" => Some("/home/alice".to_string()),
                _ => None,
            },
            false,
        )
        .unwrap();

        assert_eq!(dir, PathBuf::from("/tmp/xdg-config"));
    }

    #[test]
    fn test_config_dir_from_env_unix_falls_back_to_home() {
        let dir = config_dir_from_env(
            |key| match key {
                "HOME" => Some("/home/alice".to_string()),
                _ => None,
            },
            false,
        )
        .unwrap();

        assert_eq!(dir, PathBuf::from("/home/alice/.config"));
    }

    #[test]
    fn test_parse_valid_plugin() {
        let plugin: Plugin = toml::from_str(sample_toml()).unwrap();
        assert_eq!(plugin.name, "skip-cookie-banner");
        assert_eq!(
            plugin.description.as_deref(),
            Some("Auto close cookie consent popup")
        );
        assert_eq!(plugin.match_pattern, "*.example.com/*");
        assert_eq!(plugin.trigger, "on_load");
        assert_eq!(plugin.steps.len(), 2);
    }

    #[test]
    fn test_parse_optional_description_missing() {
        let plugin: Plugin = toml::from_str(sample_toml_no_description()).unwrap();
        assert_eq!(plugin.name, "simple-plugin");
        assert!(plugin.description.is_none());
    }

    #[test]
    fn test_parse_multiple_steps() {
        let plugin: Plugin = toml::from_str(sample_toml()).unwrap();
        let step0 = &plugin.steps[0];
        assert_eq!(step0.wait.as_deref(), Some("button:contains('Accept')"));
        assert_eq!(step0.timeout, Some(3000));
        assert_eq!(step0.action, "click");
        assert!(step0.value.is_none());

        let step1 = &plugin.steps[1];
        assert_eq!(step1.wait.as_deref(), Some("500"));
        assert!(step1.timeout.is_none());
        assert_eq!(step1.action, "wait");
    }

    #[test]
    fn test_url_matching_exact() {
        let re = glob_to_regex("https://example.com/page").unwrap();
        assert!(re.is_match("https://example.com/page"));
        assert!(!re.is_match("https://example.com/other"));
    }

    #[test]
    fn test_url_matching_wildcard() {
        let re = glob_to_regex("*.example.com/*").unwrap();
        assert!(re.is_match("www.example.com/page"));
        assert!(re.is_match("sub.example.com/anything"));
        assert!(!re.is_match("example.com/page")); // no subdomain prefix
    }

    #[test]
    fn test_url_matching_no_match() {
        let re = glob_to_regex("*.example.com/*").unwrap();
        assert!(!re.is_match("www.other.com/page"));
    }

    #[test]
    fn test_load_plugin_from_path() {
        let dir = std::env::temp_dir().join(format!("browser-cli-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test-plugin.toml");
        fs::write(&path, sample_toml()).unwrap();

        let plugin = load_plugin_from_path(&path).unwrap();
        assert_eq!(plugin.name, "skip-cookie-banner");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_list_plugins_empty_dir() {
        let dir = std::env::temp_dir().join(format!("browser-cli-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        // list_plugins uses the real config dir, so just ensure it doesn't panic
        // when the directory is empty or doesn't exist.
        let _ = list_plugins();

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_find_matching_plugins_via_parse() {
        // Test the matching logic directly since find_matching_plugins depends on
        // the real plugin directory.
        let plugin: Plugin = toml::from_str(sample_toml()).unwrap();
        let re = glob_to_regex(&plugin.match_pattern).unwrap();

        assert!(re.is_match("www.example.com/cookies"));
        assert!(!re.is_match("www.other.com/cookies"));
    }
}
