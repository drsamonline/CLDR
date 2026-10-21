//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! Zero-dependency extras, kept out of the hot path:
//!
//!   * Config      — `~/.config/cldr/config.toml` parsed by a tiny hand-rolled
//!                   key=value reader (no `toml` crate, no allocations on the
//!                   lookup path beyond the once-per-process load)
//!   * Aliases     — `alias.g = /web`, expanded one level deep before routing
//!   * History     — `~/.config/cldr/history`, capped and de-duplicated
//!   * convert_units — length / mass / temperature, wired into the cascade
//!   * base64      — for OSC 52 clipboard copy (no clipboard crate)
//!   * self_test   — `cldr --self-test`, headless sanity check for CI

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::engine::{calc, fmt_num, urlencode};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Config {
    /// Search URL template; `{q}` is replaced with the URL-encoded query.
    pub search_url: String,
    /// Row cap for `/ls` and `/find`.
    pub max_results: usize,
    /// Directory depth cap for `/find`.
    pub find_depth: usize,
    /// `name -> replacement` command aliases.
    pub aliases: Vec<(String, String)>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            search_url: "https://duckduckgo.com/?q={q}".to_string(),
            max_results: 50,
            find_depth: 4,
            aliases: Vec::new(),
        }
    }
}

impl Config {
    pub fn alias(&self, name: &str) -> Option<&str> {
        self.aliases
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// `~/.config/cldr` on Unix, `%APPDATA%\cldr` on Windows.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("cldr"));
    }
    #[cfg(not(windows))]
    {
        if let Some(x) = env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(x).join("cldr"));
        }
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|p| p.join(".config").join("cldr"));
    }
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

pub fn history_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("history"))
}

/// Parse the config file. Unknown keys and malformed lines are ignored, so a
/// hand-edited file can never break startup.
pub fn load_config() -> Config {
    let mut cfg = Config::default();
    let Some(path) = config_path() else { return cfg };
    let Ok(text) = fs::read_to_string(&path) else { return cfg };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..]
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim();
        if val.is_empty() {
            continue;
        }
        match key {
            "search_url" | "search" => cfg.search_url = val.to_string(),
            "max_results" => {
                if let Ok(n) = val.parse::<usize>() {
                    cfg.max_results = n.clamp(1, 1000);
                }
            }
            "find_depth" => {
                if let Ok(n) = val.parse::<usize>() {
                    cfg.find_depth = n.clamp(1, 32);
                }
            }
            _ => {
                if let Some(name) = key.strip_prefix("alias.") {
                    let name = name.trim();
                    if !name.is_empty() {
                        cfg.aliases.push((name.to_string(), val.to_string()));
                    }
                }
            }
        }
    }
    cfg
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Process-wide config, loaded once on first use.
pub fn config() -> &'static Config {
    CONFIG.get_or_init(load_config)
}

/// Write a commented starter config if none exists yet. Best effort.
pub fn ensure_config() -> Option<PathBuf> {
    let path = config_path()?;
    if path.exists() {
        return Some(path);
    }
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
    }
    let body = "\
# CLDR config — © 2026 Dr. Sohil Momin (drsamonline). All values optional.
# search_url: {q} is replaced by your URL-encoded query.
# search_url = \"https://duckduckgo.com/?q={q}\"

# Row caps
# max_results = 50
# find_depth  = 4

# Aliases: \"name = replacement\". \"name arg\" appends arg after replacement.
# alias.g  = /web
# alias.gh = /web github.com
# alias.n  = /run nvim
";
    if fs::write(&path, body).is_ok() {
        Some(path)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

const HISTORY_CAP: usize = 500;
const HISTORY_MAX_BYTES: u64 = 64_000;

pub fn load_history() -> Vec<String> {
    let Some(p) = history_path() else {
        return Vec::new();
    };
    let Ok(t) = fs::read_to_string(p) else {
        return Vec::new();
    };
    let mut v: Vec<String> = t
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if v.len() > HISTORY_CAP {
        v.drain(..v.len() - HISTORY_CAP);
    }
    v
}

/// Append one command to the persistent history, de-duplicating consecutive
/// entries and rotating the file so it cannot grow without bound.
pub fn append_history(line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let Some(dir) = config_dir() else { return };
    let _ = fs::create_dir_all(&dir);
    let Some(p) = history_path() else { return };

    if let Ok(existing) = fs::read_to_string(&p) {
        if existing.lines().last().map(str::trim) == Some(line) {
            return;
        }
        if existing.len() as u64 > HISTORY_MAX_BYTES {
            let kept: Vec<&str> = existing.lines().rev().take(HISTORY_CAP).collect();
            let mut out = kept.into_iter().rev().collect::<Vec<_>>().join("\n");
            out.push('\n');
            let _ = fs::write(&p, out);
        }
    }

    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{line}");
    }
}

// ---------------------------------------------------------------------------
// Unit conversion  (length / mass / temperature)
// ---------------------------------------------------------------------------

const LEN: &[(&str, f64)] = &[
    ("mm", 0.001),
    ("cm", 0.01),
    ("m", 1.0),
    ("km", 1000.0),
    ("in", 0.0254),
    ("ft", 0.3048),
    ("yd", 0.9144),
    ("mi", 1609.344),
    ("nmi", 1852.0),
];

const MASS: &[(&str, f64)] = &[
    ("mg", 0.001),
    ("g", 1.0),
    ("kg", 1000.0),
    ("oz", 28.349_523_125),
    ("lb", 453.592_37),
    ("st", 6350.293_18),
    ("t", 1_000_000.0),
];

/// Normalise a user-typed unit name to a canonical short code.
fn canon(u: &str) -> &str {
    match u {
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => "mm",
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => "cm",
        "m" | "meter" | "meters" | "metre" | "metres" => "m",
        "km" | "kilometer" | "kilometers" | "kilometre" | "kilometres" => "km",
        "in" | "inch" | "inches" => "in",
        "ft" | "foot" | "feet" => "ft",
        "yd" | "yard" | "yards" => "yd",
        "mi" | "mile" | "miles" => "mi",
        "nmi" | "nauticalmile" | "nauticalmiles" => "nmi",
        "mg" | "milligram" | "milligrams" => "mg",
        "g" | "gram" | "grams" | "gramme" | "grammes" => "g",
        "kg" | "kilogram" | "kilograms" | "kilo" | "kilos" => "kg",
        "oz" | "ounce" | "ounces" => "oz",
        "lb" | "lbs" | "pound" | "pounds" => "lb",
        "st" | "stone" | "stones" => "st",
        "t" | "tonne" | "tonnes" | "ton" | "tons" => "t",
        "c" | "celsius" | "centigrade" => "c",
        "f" | "fahrenheit" => "f",
        "k" | "kelvin" => "k",
        other => other,
    }
}

fn factor(u: &str) -> Option<f64> {
    for (k, v) in LEN.iter().chain(MASS.iter()) {
        if *k == u {
            return Some(*v);
        }
    }
    None
}

fn to_celsius(v: f64, from: &str) -> f64 {
    match from {
        "f" => (v - 32.0) * 5.0 / 9.0,
        "k" => v - 273.15,
        _ => v,
    }
}

fn from_celsius(c: f64, to: &str) -> f64 {
    match to {
        "f" => c * 9.0 / 5.0 + 32.0,
        "k" => c + 273.15,
        _ => c,
    }
}

/// Split a leading numeric literal from its trailing unit, e.g. `10km` → ("10", "km").
fn split_num_unit(s: &str) -> Option<(String, String)> {
    let b = s.as_bytes();
    let mut i = 0usize;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.' || b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let num = s[..i].trim().to_string();
    let unit = s[i..].trim().split_whitespace().next()?.to_string();
    Some((num, unit))
}

/// Convert a natural-language-ish expression such as `10 km in mi`,
/// `100 f to c`, `5kg to lb`. Returns `None` when it is not a conversion.
pub fn convert_units(s: &str) -> Option<String> {
    let low = s.trim().to_lowercase();
    let connectors = [" in ", " to ", " as ", " -> ", " => ", " = "];
    let mut best = usize::MAX;
    let mut split: Option<(&str, &str)> = None;
    for c in connectors {
        if let Some(i) = low.find(c) {
            if i < best {
                best = i;
                split = Some((&low[..i], &low[i + c.len()..]));
            }
        }
    }
    let (left, right) = split?;
    let to = canon(right.trim().split_whitespace().next()?);
    let (num, from_raw) = split_num_unit(left.trim())?;
    let from = canon(&from_raw);
    let value: f64 = num.parse().ok()?;

    let temps = ["c", "f", "k"];
    if temps.contains(&from) || temps.contains(&to) {
        if !(temps.contains(&from) && temps.contains(&to)) {
            return None;
        }
        let out = from_celsius(to_celsius(value, from), to);
        return Some(format!("{s} = {}", fmt_num(out)));
    }

    let from_f = factor(from)?;
    let to_f = factor(to)?;
    let out = value * from_f / to_f;
    Some(format!("{s} = {}", fmt_num(out)))
}

// ---------------------------------------------------------------------------
// base64 (for OSC 52 clipboard writes — no clipboard crate)
// ---------------------------------------------------------------------------

pub fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Self-test (headless, for CI on machines with no terminal)
// ---------------------------------------------------------------------------

pub fn self_test() -> i32 {
    let mut fails = 0i32;
    macro_rules! check {
        ($name:expr, $cond:expr) => {{
            let ok = $cond;
            println!("[{}] {}", if ok { "PASS" } else { "FAIL" }, $name);
            if !ok {
                fails += 1;
            }
        }};
    }

    check!("calc precedence", calc("1+2*3").map(|v| v == 7.0).unwrap_or(false));
    check!("calc power right-assoc", calc("2^3^2").map(|v| v == 512.0).unwrap_or(false));
    check!("calc unary exponent", calc("2^-1").map(|v| v == 0.5).unwrap_or(false));
    check!("calc div-by-zero rejected", calc("1/0").is_err());
    check!("calc modulo-by-zero rejected", calc("5 % 0").is_err());
    check!("fmt_num integer", fmt_num(6.0) == "6");
    check!("fmt_num scientific", fmt_num(2f64.powi(1000)).contains('e'));
    check!("fmt_num NaN", fmt_num(f64::NAN) == "NaN");
    check!("urlencode", urlencode("a b&c") == "a+b%26c");
    check!("base64", base64(b"hi") == "aGk=");
    check!("convert km->mi", convert_units("10 km in mi").is_some());
    check!("convert kg->lb", convert_units("5 kg to lb").is_some());
    check!("convert c->f", convert_units("100 c to f").is_some());
    check!("convert non-conversion is None", convert_units("hello world").is_none());

    if fails == 0 {
        println!("self-test: OK (14/14)");
        0
    } else {
        println!("self-test: {fails} FAILED");
        1
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_length() {
        let r = convert_units("10 km in mi").unwrap();
        assert!(r.contains("6.21"), "got {r}");
        assert!(convert_units("1 m to cm").unwrap().contains("100"));
    }

    #[test]
    fn conversion_mass() {
        assert!(convert_units("1 kg to g").unwrap().contains("1000"));
        assert!(convert_units("5kg to lb").unwrap().contains("11.0"));
    }

    #[test]
    fn conversion_temperature() {
        assert!(convert_units("100 c to f").unwrap().contains("212"));
        assert!(convert_units("0 c to k").unwrap().contains("273.15"));
        // A temperature unit mixed with a non-temperature unit is not a conversion.
        assert!(convert_units("100 c to km").is_none());
    }

    #[test]
    fn non_conversions() {
        assert!(convert_units("hello world").is_none());
        assert!(convert_units("10 km").is_none());
        assert!(convert_units("2 + 2").is_none());
    }

    #[test]
    fn base64_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn config_defaults_are_sane() {
        let c = Config::default();
        assert!(c.search_url.contains("{q}"));
        assert!(c.max_results > 0);
        assert!(c.find_depth > 0);
    }
}
