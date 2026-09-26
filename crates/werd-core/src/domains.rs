//! `.test` domains: naming and the Werd block of the hosts file.
//!
//! The hosts file needs administrator rights, so the daemon never writes it.
//! Clients run `werd-helper` elevated, which calls [`apply_hosts`] with the
//! domains to map. The helper accepts only `*.test` names and always maps them
//! to 127.0.0.1, so even a tampered call cannot redirect a real domain.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

pub const TLD: &str = "test";
const BEGIN: &str = "# BEGIN Werd (managed automatically, do not edit)";
const END: &str = "# END Werd";

/// Turns a folder or site name into a domain label: `My Shop` → `my-shop`.
pub fn label(name: &str) -> String {
    let mut label = String::new();
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            label.push(character);
        } else if !label.is_empty() && !label.ends_with('-') {
            label.push('-');
        }
    }
    let label = label.trim_end_matches('-');
    let label: String = label.chars().take(63).collect();
    let label = label.trim_end_matches('-');
    if label.is_empty() {
        "site".into()
    } else {
        label.into()
    }
}

/// A full `.test` domain Werd may manage: lowercase labels of letters, digits and dashes.
pub fn is_valid(domain: &str) -> bool {
    let Some(name) = domain.strip_suffix(".test") else {
        return false;
    };
    domain.len() <= 253
        && !name.is_empty()
        && name.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// Normalizes user input (`Shop`, `shop.test`) to a valid domain.
pub fn normalize(input: &str) -> Result<String> {
    let input = input.trim().to_lowercase();
    let domain = if input.ends_with(".test") {
        input
    } else {
        format!("{input}.test")
    };
    if !is_valid(&domain) {
        bail!("{domain} is not a valid .test domain (use letters, digits and dashes)");
    }
    Ok(domain)
}

/// The system hosts file.
pub fn hosts_path() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        PathBuf::from(root).join(r"System32\drivers\etc\hosts")
    } else {
        PathBuf::from("/etc/hosts")
    }
}

/// Domains currently in the Werd block of `contents`.
pub fn managed(contents: &str) -> Vec<String> {
    let mut inside = false;
    let mut domains = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line == BEGIN {
            inside = true;
        } else if line == END {
            inside = false;
        } else if inside {
            domains.extend(line.split_whitespace().skip(1).map(str::to_string));
        }
    }
    domains
}

/// `contents` with the Werd block replaced by `domains` (removed when empty).
/// Everything outside the block is kept byte for byte, including line endings.
pub fn render(contents: &str, domains: &[String]) -> Result<String> {
    for domain in domains {
        if !is_valid(domain) {
            bail!("Refusing to write {domain}: only .test domains are managed");
        }
    }
    let newline = if contents.contains("\r\n") { "\r\n" } else { "\n" };
    let mut kept = Vec::new();
    let mut inside = false;
    for line in contents.split_inclusive('\n') {
        match line.trim() {
            BEGIN => inside = true,
            END => inside = false,
            _ if !inside => kept.push(line),
            _ => {}
        }
    }
    let mut output: String = kept.concat();
    if domains.is_empty() {
        return Ok(output);
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push_str(newline);
    }
    output.push_str(BEGIN);
    output.push_str(newline);
    for domain in domains {
        output.push_str(&format!("127.0.0.1 {domain}{newline}"));
    }
    output.push_str(END);
    output.push_str(newline);
    Ok(output)
}

/// Writes `domains` into the hosts file. Needs administrator rights.
pub fn apply_hosts(domains: &[String]) -> Result<()> {
    let path = hosts_path();
    let contents = fs::read_to_string(&path).with_context(|| format!("Cannot read {}", path.display()))?;
    let updated = render(&contents, domains)?;
    if updated != contents {
        fs::write(&path, updated).with_context(|| format!("Cannot write {}", path.display()))?;
    }
    Ok(())
}

/// Domains of `wanted` missing from the hosts file.
pub fn missing_from_hosts(wanted: &[String]) -> Vec<String> {
    let present = fs::read_to_string(hosts_path())
        .map(|contents| managed(&contents))
        .unwrap_or_default();
    wanted
        .iter()
        .filter(|domain| !present.contains(domain))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_derived_from_names() {
        assert_eq!(label("My Shop"), "my-shop");
        assert_eq!(label("laravel_12--api"), "laravel-12-api");
        assert_eq!(label("Café!"), "caf");
        assert_eq!(label("---"), "site");
        assert_eq!(label(&"a".repeat(80)).len(), 63);
    }

    #[test]
    fn only_test_domains_are_valid() {
        for valid in ["shop.test", "api.shop.test", "a-1.test"] {
            assert!(is_valid(valid), "{valid}");
        }
        for invalid in [
            "shop.com",
            ".test",
            "-shop.test",
            "Shop.test",
            "shop..test",
            "evil.com shop.test",
            "shop.test\n1.2.3.4 bank.com",
        ] {
            assert!(!is_valid(invalid), "{invalid}");
        }
        assert_eq!(normalize(" Shop ").unwrap(), "shop.test");
        assert_eq!(normalize("api.shop.test").unwrap(), "api.shop.test");
        assert!(normalize("my shop").is_err());
    }

    #[test]
    fn the_block_is_added_replaced_and_removed() {
        let original = "# comment\r\n127.0.0.1 localhost\r\n";
        let one = render(original, &["shop.test".into()]).unwrap();
        assert_eq!(
            one,
            format!("{original}{BEGIN}\r\n127.0.0.1 shop.test\r\n{END}\r\n")
        );
        assert_eq!(managed(&one), ["shop.test"]);

        let two = render(&one, &["blog.test".into(), "shop.test".into()]).unwrap();
        assert_eq!(managed(&two), ["blog.test", "shop.test"]);
        assert!(two.starts_with(original));

        assert_eq!(render(&two, &[]).unwrap(), original);
        assert!(render(original, &["bank.com".into()]).is_err());
    }

    #[test]
    fn a_file_without_final_newline_is_kept_intact() {
        let rendered = render("127.0.0.1 localhost", &["a.test".into()]).unwrap();
        assert!(rendered.starts_with("127.0.0.1 localhost\n# BEGIN"));
    }
}
