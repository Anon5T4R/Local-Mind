//! The window's arguments: the map to open (`Open with` and the command line pass it), `--control
//! PORT` and the automation roots that confine what its control channel may touch.

use craft_mcp::automation::RootFlags;

/// What the window was started with.
#[derive(Debug, Default)]
pub struct Launch {
    /// Everything else, in order: the map path and `--help`/`--version`, which [`crate::main`]
    /// reads as before.
    pub args: Vec<String>,
    /// The loopback control channel's port (`--control PORT`).
    pub control: Option<u16>,
    /// The folders the control channel's commands may read and write.
    pub roots: RootFlags,
}

/// Split `--control` and the suite's root flags out of `args`; the rest goes on untouched.
pub fn parse(args: &[String]) -> Result<Launch, String> {
    let (roots, rest) = RootFlags::take(args)?;
    let mut kept = Vec::with_capacity(rest.len());
    let mut control = None;
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--control" => control = Some(port(it.next().ok_or("--control needs a port")?)?),
            other if other.starts_with("--control=") => control = Some(port(other.trim_start_matches("--control="))?),
            other => kept.push(other.to_string()),
        }
    }
    if control.is_none() && !roots.is_empty() {
        return Err("--automation-read-root and --automation-write-root confine the control channel: start the app with `--control PORT`".into());
    }
    Ok(Launch { args: kept, control, roots })
}

fn port(raw: &str) -> Result<u16, String> {
    let port: u16 = raw.parse().map_err(|_| format!("--control {raw}: a port is 1 to 65535"))?;
    if port == 0 {
        return Err(format!("--control {raw}: a port is 1 to 65535"));
    }
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_map_and_the_help_flags_are_left_for_the_window_parser() {
        let launch = parse(&args(&["a.tmind"])).expect("a map");
        assert_eq!(launch.args, ["a.tmind"]);
        assert!(launch.control.is_none());
    }

    #[test]
    fn control_takes_a_port_and_may_come_before_the_map() {
        let launch = parse(&args(&["--control", "7979", "a.tmind"])).expect("a control port");
        assert_eq!(launch.control, Some(7979));
        assert_eq!(launch.args, ["a.tmind"]);
        assert_eq!(parse(&args(&["--control=7980"])).expect("inline").control, Some(7980));
        for bad in ["0", "65536", "abc", "-1"] {
            assert!(parse(&args(&["--control", bad])).is_err(), "{bad}");
        }
        assert!(parse(&args(&["--control"])).is_err());
    }

    #[test]
    fn the_roots_belong_to_the_control_channel() {
        let launch = parse(&args(&["--control", "7979", "--automation-read-root", "in", "--automation-write-root=out"])).expect("roots with control");
        assert_eq!(launch.roots.read, ["in"]);
        assert_eq!(launch.roots.write, ["out"]);
        let e = parse(&args(&["--automation-write-root", "out"])).unwrap_err();
        assert!(e.contains("confine the control channel") && e.contains("--control"), "{e}");
    }
}
