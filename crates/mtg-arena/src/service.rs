//! Running the recorder in the background on macOS, as a launchd user agent
//! that starts at login and is restarted if it exits.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

pub const LABEL: &str = "com.mtg-engine.arena-recorder";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The launchd property list that runs `program` with `args`, appending its
/// output to `log`.
pub fn plist(program: &Path, args: &[String], log: &Path) -> String {
    let mut program_args = format!(
        "    <string>{}</string>\n",
        xml_escape(&program.to_string_lossy())
    );
    for a in args {
        program_args.push_str("    <string>");
        program_args.push_str(&xml_escape(a));
        program_args.push_str("</string>\n");
    }
    let log = xml_escape(&log.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
{program_args}  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>ThrottleInterval</key>
  <integer>30</integer>
  <key>ProcessType</key>
  <string>Background</string>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
</dict>
</plist>
"#
    )
}

pub fn plist_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

fn domain() -> Result<String> {
    let out = Command::new("id")
        .arg("-u")
        .output()
        .context("running id -u")?;
    Ok(format!(
        "gui/{}",
        String::from_utf8_lossy(&out.stdout).trim()
    ))
}

fn launchctl(args: &[&str]) -> Result<std::process::Output> {
    Command::new("launchctl")
        .args(args)
        .output()
        .context("running launchctl")
}

fn require_macos() -> Result<()> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        bail!("the background service is only implemented for macOS (launchd)")
    }
}

/// Install and start the agent. Replaces an existing installation.
pub fn install(program: &Path, args: &[String], log: &Path) -> Result<PathBuf> {
    require_macos()?;
    let path = plist_path()?;
    let domain = domain()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Stop a previous installation first; failure means there was none.
    let _ = launchctl(&["bootout", &format!("{domain}/{LABEL}")]);
    std::fs::write(&path, plist(program, args, log))
        .with_context(|| format!("writing {}", path.display()))?;
    let out = launchctl(&["bootstrap", &domain, &path.to_string_lossy()])?;
    if !out.status.success() {
        bail!(
            "launchctl bootstrap failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(path)
}

/// Stop the agent and remove its property list. Returns whether one was
/// installed.
pub fn uninstall() -> Result<bool> {
    require_macos()?;
    let path = plist_path()?;
    let _ = launchctl(&["bootout", &format!("{}/{LABEL}", domain()?)]);
    if path.exists() {
        std::fs::remove_file(&path)?;
        return Ok(true);
    }
    Ok(false)
}

/// The agent's state as launchd reports it, or `None` if it is not loaded.
pub fn status() -> Result<Option<String>> {
    require_macos()?;
    let out = launchctl(&["print", &format!("{}/{LABEL}", domain()?)])?;
    if !out.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let wanted = ["state =", "pid =", "program =", "last exit code ="];
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| wanted.iter().any(|w| l.starts_with(w)))
        .collect();
    Ok(Some(lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_escapes_and_lists_arguments() {
        let p = plist(
            Path::new("/bin/mtg"),
            &[
                "arena".into(),
                "watch".into(),
                "--store".into(),
                "/a & b".into(),
            ],
            Path::new("/tmp/r.log"),
        );
        assert!(p.contains("<string>/bin/mtg</string>\n    <string>arena</string>"));
        assert!(p.contains("<string>/a &amp; b</string>"));
        assert!(p.contains(&format!("<string>{LABEL}</string>")));
        assert_eq!(p.matches("/tmp/r.log").count(), 2);
    }
}
