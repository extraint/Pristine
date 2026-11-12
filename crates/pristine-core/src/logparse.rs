//! Classify lines of Minecraft/Java output for the live log.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub n: u64,
    /// `HH:MM:SS` taken from the game's own prefix, or empty.
    pub ts: String,
    /// error | warn | info | debug
    pub level: &'static str,
    pub text: String,
}

fn timestamp(line: &str) -> Option<&str> {
    let t = line.strip_prefix('[')?.get(..8)?;
    let b = t.as_bytes();
    let ok = b[2] == b':' && b[5] == b':' && [0, 1, 3, 4, 6, 7].iter().all(|&i| b[i].is_ascii_digit());
    ok.then_some(t)
}

pub fn classify(line: &str, from_stderr: bool) -> (String, &'static str) {
    let ts = timestamp(line).unwrap_or("").to_string();
    let level = if line.contains("/FATAL]") || line.contains("/ERROR]") { "error" }
        else if line.contains("/WARN]") { "warn" }
        else if line.contains("/DEBUG]") || line.contains("/TRACE]") { "debug" }
        else if line.contains("/INFO]") { "info" }
        else if line.starts_with("Caused by:") || line.starts_with("Exception") || line.starts_with("\tat ")
            || line.starts_with("java.lang.") || line.contains("Exception in thread") { "error" }
        else if line.starts_with("WARNING") { "warn" }
        else if from_stderr { "warn" }
        else { "info" };
    (ts, level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_lines() {
        assert_eq!(classify("[12:34:56] [main/INFO]: Setting user: Steve", false), ("12:34:56".into(), "info"));
        assert_eq!(classify("[01:02:03] [Render thread/WARN]: Ambiguity", false).1, "warn");
        assert_eq!(classify("[01:02:03] [main/ERROR]: boom", false).1, "error");
        assert_eq!(classify("[01:02:03] [main/DEBUG]: x", false).1, "debug");
    }

    #[test]
    fn untagged_lines() {
        assert_eq!(classify("\tat net.minecraft.Foo.bar(Foo.java:1)", false).1, "error");
        assert_eq!(classify("Caused by: java.lang.Oops", false).1, "error");
        assert_eq!(classify("some jvm chatter", true).1, "warn");
        assert_eq!(classify("hello", false), (String::new(), "info"));
        assert_eq!(classify("[xx:yy:zz] nope", false).0, "");
    }
}
