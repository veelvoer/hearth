//! The friendly text screen: colours, wrapping, steps, and the questions (which can be scripted for tests).
use std::io::{BufRead, Write};

pub const FLAME: [&str; 6] = ["      (  )", "     (    )", "    (  /\\  )", "    ( /  \\ )", "     \\____/", "  ____||____"];

pub fn tty() -> bool {
    // SAFETY: isatty only reads a file descriptor number.
    let a = unsafe { libc::isatty(1) == 1 };
    a && std::env::var_os("NO_COLOR").is_none()
}
fn c(code: &str, s: &str) -> String { if tty() { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() } }
pub fn bold(s: &str) -> String { c("1", s) }
pub fn dim(s: &str) -> String { c("2", s) }
pub fn ember(s: &str) -> String { c("38;5;208", s) }
pub fn green(s: &str) -> String { c("32", s) }
pub fn red(s: &str) -> String { c("31", s) }
pub fn yellow(s: &str) -> String { c("33", s) }

pub fn width() -> usize { std::env::var("COLUMNS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(80).min(78) }

/// Word wrap; every paragraph (split on \n) is wrapped on its own and every line gets `indent`.
pub fn wrap(text: &str, indent: &str, width: usize) -> String {
    text.split('\n').map(|para| {
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for w in para.split(' ') {
            let joined = format!("{cur} {w}");
            if joined.trim().chars().count() > width { lines.push(std::mem::take(&mut cur)); cur = w.to_string(); } else { cur = joined.trim().to_string(); }
        }
        lines.push(cur);
        lines.iter().map(|l| format!("{indent}{l}")).collect::<Vec<_>>().join("\n")
    }).collect::<Vec<_>>().join("\n")
}
pub fn say(t: &str) { println!("{}", wrap(t, "  ", width().saturating_sub(4))); }
pub fn gap() { println!(); }
pub fn item(n: usize, text: &str) {
    let w = wrap(text, "", width().saturating_sub(9));
    let mut it = w.split('\n');
    println!("  {}  {}", ember(&format!("{n}.")), it.next().unwrap_or(""));
    for l in it { println!("      {l}"); }
}
pub fn banner(version: &str) {
    println!();
    for l in FLAME { println!("{}", ember(l)); }
    println!("{}{}", bold("  Hearth server"), dim(&format!("  ·  version {version}")));
    gap();
}
pub fn step(n: usize, total: usize, title: &str) {
    gap();
    println!("{}{}", ember(&format!("  Step {n} of {total}")), bold(&format!("  {title}")));
    println!("{}", dim(&format!("  {}", "─".repeat(60.min(width().saturating_sub(4))))));
    gap();
}
pub fn ok(t: &str) { println!("  {}{t}", green("✓ ")); }
pub fn warn(t: &str) { println!("  {}{t}", yellow("! ")); }
pub fn bad(t: &str) { println!("  {}{t}", red("✗ ")); }

/// Questions. With `yes` every question returns its default; tests give the answers as text lines.
pub struct Prompt { pub yes: bool, input: Box<dyn BufRead + Send> }
impl Prompt {
    pub fn stdin(yes: bool) -> Self { Prompt { yes, input: Box::new(std::io::BufReader::new(std::io::stdin())) } }
    pub fn scripted(yes: bool, lines: &str) -> Self { Prompt { yes, input: Box::new(std::io::Cursor::new(lines.as_bytes().to_vec())) } }
    pub fn ask(&mut self, q: &str, def: &str) -> String {
        if self.yes { return def.to_string(); }
        print!("  {}{} {}", bold(q), if def.is_empty() { String::new() } else { dim(&format!(" ({def})")) }, ember("› "));
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        let n = self.input.read_line(&mut line).unwrap_or(0);
        if n == 0 { return def.to_string(); }   // end of input: take the easy choice, never loop forever
        let a = line.trim();
        if a.is_empty() { def.to_string() } else { a.to_string() }
    }
    /// Shows numbered options and returns the zero-based index.
    pub fn choose(&mut self, q: &str, options: &[(&str, &str)], def: usize) -> usize {
        say(&bold(q)); gap();
        for (i, (label, hint)) in options.iter().enumerate() {
            println!("  {}  {}{}", ember(&format!("{}", i + 1)), bold(label), if i == def { dim("   ← the easy choice") } else { String::new() });
            if !hint.is_empty() { println!("{}", dim(&wrap(hint, "     ", width().saturating_sub(8)))); }
        }
        gap();
        if self.yes { return def; }
        loop {
            let a = self.ask("Type a number and press Enter", &(def + 1).to_string());
            if let Ok(n) = a.parse::<usize>() { if n >= 1 && n <= options.len() { return n - 1; } }
            if self.eof() { return def; }
            bad("Please type one of the numbers above.");
        }
    }
    pub fn confirm(&mut self, q: &str, def: bool) -> bool {
        if self.yes { return def; }
        loop {
            let a = self.ask(&format!("{q}{}", if def { " [Y/n]" } else { " [y/N]" }), "").to_lowercase();
            if a.is_empty() { return def; }
            if a.starts_with('y') { return true; }
            if a.starts_with('n') { return false; }
            if self.eof() { return def; }
        }
    }
    pub fn pause(&mut self, msg: &str) { if !self.yes { let _ = self.ask(msg, ""); } }
    fn eof(&mut self) -> bool { self.input.fill_buf().map(|b| b.is_empty()).unwrap_or(true) }
}
