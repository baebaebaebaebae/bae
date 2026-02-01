use std::fmt::Write;
use std::path::PathBuf;

fn crash_log_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".bae").join("crash.log"))
}

/// Get the ASLR slide for the main executable.
///
/// Combined with the raw addresses in the backtrace, this lets you
/// symbolicate offline: `atos -o bae.dSYM -s <slide> <addr1> <addr2> ...`
#[cfg(target_os = "macos")]
fn aslr_slide() -> isize {
    extern "C" {
        fn _dyld_get_image_vmaddr_slide(image_index: u32) -> isize;
    }
    // SAFETY: Index 0 is always the main executable. Read-only query.
    unsafe { _dyld_get_image_vmaddr_slide(0) }
}

#[cfg(not(target_os = "macos"))]
fn aslr_slide() -> isize {
    0
}

/// Capture backtrace as raw instruction pointer addresses.
///
/// `std::backtrace::Backtrace` resolves symbols at capture time, which
/// produces useless output in stripped release builds (every frame shows as
/// `__mh_execute_header`). By recording raw addresses instead, the report
/// can be symbolicated offline against the .dSYM bundle using `atos`.
fn capture_backtrace() -> String {
    let mut out = String::new();
    let mut frame_no = 0usize;

    backtrace::trace(|frame| {
        let ip = frame.ip() as usize;
        let _ = writeln!(out, "  {frame_no:>3}: 0x{ip:016x}");
        frame_no += 1;
        true
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backtrace_contains_hex_addresses() {
        let bt = capture_backtrace();
        let lines: Vec<&str> = bt.lines().collect();

        assert!(
            !lines.is_empty(),
            "backtrace should have at least one frame"
        );

        for line in &lines {
            assert!(
                line.contains("0x"),
                "frame should contain a hex address: {line}"
            );
            assert!(
                !line.contains("__mh_execute_header"),
                "frame should be a raw address, not an unresolved symbol: {line}"
            );
        }
    }
}

pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = (|| -> std::io::Result<()> {
            let path = match crash_log_path() {
                Some(p) => p,
                None => return Ok(()),
            };

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };

            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "unknown".to_string());

            let backtrace = capture_backtrace();
            let slide = aslr_slide();
            let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
            let version = env!("BAE_VERSION");

            let report = format!(
                "bae crash report\n================\nTime: {now}\nVersion: {version}\nSlide: 0x{slide:x}\n\nPanic: {message}\nLocation: {location}\n\nBacktrace:\n{backtrace}",
            );

            std::fs::write(&path, report)?;
            Ok(())
        })();

        default_hook(info);
    }));
}

pub fn check_for_crash_report() {
    let path = match crash_log_path() {
        Some(p) if p.exists() => p,
        _ => return,
    };

    let report = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(_) => return,
    };

    let _ = std::fs::remove_file(&path);

    let should_report = rfd::MessageDialog::new()
        .set_title("bae crashed")
        .set_description("bae crashed during the last session. Would you like to open a GitHub issue with the crash report?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();

    if should_report == rfd::MessageDialogResult::Yes {
        // Truncate report for URL length limits
        let truncated: String = report.chars().take(4000).collect();
        let body = format!(
            "<details>\n<summary>Crash report</summary>\n\n```\n{truncated}\n```\n\n</details>"
        );
        let url = format!(
            "https://github.com/bae-fm/bae/issues/new?title={}&body={}&labels=crash",
            urlencoding::encode("Crash report"),
            urlencoding::encode(&body),
        );

        let _ = std::process::Command::new("open").arg(&url).spawn();
    }
}
