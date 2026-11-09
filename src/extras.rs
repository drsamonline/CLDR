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
//!   * convert_units — ten dimensions (length, mass, time, volume, area, speed,
//!                   data, pressure, energy, temperature), wired into the cascade
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

/// Which quantity a unit measures. Two units convert only when the dimension
/// matches; temperature is the one affine case, handled separately.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dim {
    Len,
    Mass,
    Time,
    Vol,
    Area,
    Speed,
    Data,
    Press,
    Energy,
    Temp,
}

/// A resolved unit: its dimension, its factor to that dimension's base unit,
/// and a canonical code (temperature only — it picks the affine formula).
#[derive(Clone, Copy)]
struct Unit {
    dim: Dim,
    factor: f64,
    code: &'static str,
}

/// Every accepted spelling, lowercase (input is lowercased before lookup).
///
/// Base units: length → metre, mass → kilogram, time → second, volume → litre,
/// area → square metre, speed → metre per second, data → byte, pressure →
/// pascal, energy → joule. Data is decimal (1 kB = 1000 B) with the binary
/// forms spelled out separately (`KiB`, `MiB`, …), the same split Numbat makes.
///
/// The factors follow the definitions in Numbat's unit module
/// (github.com/sharkdp/numbat, MIT/Apache-2.0). Only the physical constants are
/// reused; no code is taken.
const UNITS: &[(&str, Dim, f64, &str)] = &[
    // length — base: metre
    ("mm", Dim::Len, 0.001, "mm"),
    ("millimeter", Dim::Len, 0.001, "mm"),
    ("millimeters", Dim::Len, 0.001, "mm"),
    ("millimetre", Dim::Len, 0.001, "mm"),
    ("millimetres", Dim::Len, 0.001, "mm"),
    ("cm", Dim::Len, 0.01, "cm"),
    ("centimeter", Dim::Len, 0.01, "cm"),
    ("centimeters", Dim::Len, 0.01, "cm"),
    ("centimetre", Dim::Len, 0.01, "cm"),
    ("centimetres", Dim::Len, 0.01, "cm"),
    ("m", Dim::Len, 1.0, "m"),
    ("meter", Dim::Len, 1.0, "m"),
    ("meters", Dim::Len, 1.0, "m"),
    ("metre", Dim::Len, 1.0, "m"),
    ("metres", Dim::Len, 1.0, "m"),
    ("km", Dim::Len, 1000.0, "km"),
    ("kilometer", Dim::Len, 1000.0, "km"),
    ("kilometers", Dim::Len, 1000.0, "km"),
    ("kilometre", Dim::Len, 1000.0, "km"),
    ("kilometres", Dim::Len, 1000.0, "km"),
    ("in", Dim::Len, 0.0254, "in"),
    ("inch", Dim::Len, 0.0254, "in"),
    ("inches", Dim::Len, 0.0254, "in"),
    ("ft", Dim::Len, 0.3048, "ft"),
    ("foot", Dim::Len, 0.3048, "ft"),
    ("feet", Dim::Len, 0.3048, "ft"),
    ("yd", Dim::Len, 0.9144, "yd"),
    ("yard", Dim::Len, 0.9144, "yd"),
    ("yards", Dim::Len, 0.9144, "yd"),
    ("mi", Dim::Len, 1609.344, "mi"),
    ("mile", Dim::Len, 1609.344, "mi"),
    ("miles", Dim::Len, 1609.344, "mi"),
    ("nmi", Dim::Len, 1852.0, "nmi"),
    ("nauticalmile", Dim::Len, 1852.0, "nmi"),
    ("nauticalmiles", Dim::Len, 1852.0, "nmi"),
    // mass — base: kilogram
    ("mg", Dim::Mass, 0.000001, "mg"),
    ("milligram", Dim::Mass, 0.000001, "mg"),
    ("milligrams", Dim::Mass, 0.000001, "mg"),
    ("g", Dim::Mass, 0.001, "g"),
    ("gram", Dim::Mass, 0.001, "g"),
    ("grams", Dim::Mass, 0.001, "g"),
    ("gramme", Dim::Mass, 0.001, "g"),
    ("grammes", Dim::Mass, 0.001, "g"),
    ("kg", Dim::Mass, 1.0, "kg"),
    ("kilogram", Dim::Mass, 1.0, "kg"),
    ("kilograms", Dim::Mass, 1.0, "kg"),
    ("kilo", Dim::Mass, 1.0, "kg"),
    ("kilos", Dim::Mass, 1.0, "kg"),
    ("t", Dim::Mass, 1000.0, "t"),
    ("tonne", Dim::Mass, 1000.0, "t"),
    ("tonnes", Dim::Mass, 1000.0, "t"),
    ("ton", Dim::Mass, 1000.0, "t"),
    ("tons", Dim::Mass, 1000.0, "t"),
    ("oz", Dim::Mass, 0.028349523125, "oz"),
    ("ounce", Dim::Mass, 0.028349523125, "oz"),
    ("ounces", Dim::Mass, 0.028349523125, "oz"),
    ("lb", Dim::Mass, 0.45359237, "lb"),
    ("lbs", Dim::Mass, 0.45359237, "lb"),
    ("pound", Dim::Mass, 0.45359237, "lb"),
    ("pounds", Dim::Mass, 0.45359237, "lb"),
    ("st", Dim::Mass, 6.35029318, "st"),
    ("stone", Dim::Mass, 6.35029318, "st"),
    ("stones", Dim::Mass, 6.35029318, "st"),
    // time — base: second
    ("ms", Dim::Time, 0.001, "ms"),
    ("millisecond", Dim::Time, 0.001, "ms"),
    ("milliseconds", Dim::Time, 0.001, "ms"),
    ("s", Dim::Time, 1.0, "s"),
    ("sec", Dim::Time, 1.0, "s"),
    ("secs", Dim::Time, 1.0, "s"),
    ("second", Dim::Time, 1.0, "s"),
    ("seconds", Dim::Time, 1.0, "s"),
    ("min", Dim::Time, 60.0, "min"),
    ("minute", Dim::Time, 60.0, "min"),
    ("minutes", Dim::Time, 60.0, "min"),
    ("h", Dim::Time, 3600.0, "h"),
    ("hr", Dim::Time, 3600.0, "h"),
    ("hrs", Dim::Time, 3600.0, "h"),
    ("hour", Dim::Time, 3600.0, "h"),
    ("hours", Dim::Time, 3600.0, "h"),
    ("d", Dim::Time, 86400.0, "d"),
    ("day", Dim::Time, 86400.0, "d"),
    ("days", Dim::Time, 86400.0, "d"),
    ("wk", Dim::Time, 604800.0, "wk"),
    ("week", Dim::Time, 604800.0, "wk"),
    ("weeks", Dim::Time, 604800.0, "wk"),
    // volume — base: litre
    ("ml", Dim::Vol, 0.001, "ml"),
    ("milliliter", Dim::Vol, 0.001, "ml"),
    ("milliliters", Dim::Vol, 0.001, "ml"),
    ("millilitre", Dim::Vol, 0.001, "ml"),
    ("millilitres", Dim::Vol, 0.001, "ml"),
    ("cl", Dim::Vol, 0.01, "cl"),
    ("dl", Dim::Vol, 0.1, "dl"),
    ("l", Dim::Vol, 1.0, "l"),
    ("liter", Dim::Vol, 1.0, "l"),
    ("liters", Dim::Vol, 1.0, "l"),
    ("litre", Dim::Vol, 1.0, "l"),
    ("litres", Dim::Vol, 1.0, "l"),
    ("m3", Dim::Vol, 1000.0, "m3"),
    ("cubicmeter", Dim::Vol, 1000.0, "m3"),
    ("cubicmetre", Dim::Vol, 1000.0, "m3"),
    ("tsp", Dim::Vol, 0.00492892159375, "tsp"),
    ("teaspoon", Dim::Vol, 0.00492892159375, "tsp"),
    ("teaspoons", Dim::Vol, 0.00492892159375, "tsp"),
    ("tbsp", Dim::Vol, 0.01478676478125, "tbsp"),
    ("tablespoon", Dim::Vol, 0.01478676478125, "tbsp"),
    ("tablespoons", Dim::Vol, 0.01478676478125, "tbsp"),
    ("floz", Dim::Vol, 0.0295735295625, "floz"),
    ("cup", Dim::Vol, 0.2365882365, "cup"),
    ("cups", Dim::Vol, 0.2365882365, "cup"),
    ("pint", Dim::Vol, 0.473176473, "pint"),
    ("pints", Dim::Vol, 0.473176473, "pint"),
    ("quart", Dim::Vol, 0.946352946, "quart"),
    ("quarts", Dim::Vol, 0.946352946, "quart"),
    ("gal", Dim::Vol, 3.785411784, "gal"),
    ("gallon", Dim::Vol, 3.785411784, "gal"),
    ("gallons", Dim::Vol, 3.785411784, "gal"),
    // area — base: square metre
    ("m2", Dim::Area, 1.0, "m2"),
    ("sqm", Dim::Area, 1.0, "m2"),
    ("squaremeter", Dim::Area, 1.0, "m2"),
    ("squaremetre", Dim::Area, 1.0, "m2"),
    ("km2", Dim::Area, 1000000.0, "km2"),
    ("sqkm", Dim::Area, 1000000.0, "km2"),
    ("cm2", Dim::Area, 0.0001, "cm2"),
    ("sqcm", Dim::Area, 0.0001, "cm2"),
    ("ha", Dim::Area, 10000.0, "ha"),
    ("hectare", Dim::Area, 10000.0, "ha"),
    ("hectares", Dim::Area, 10000.0, "ha"),
    ("acre", Dim::Area, 4046.8564224, "acre"),
    ("acres", Dim::Area, 4046.8564224, "acre"),
    ("ft2", Dim::Area, 0.09290304, "ft2"),
    ("sqft", Dim::Area, 0.09290304, "ft2"),
    ("yd2", Dim::Area, 0.83612736, "yd2"),
    ("sqyd", Dim::Area, 0.83612736, "yd2"),
    ("mi2", Dim::Area, 2589988.110336, "mi2"),
    ("sqmi", Dim::Area, 2589988.110336, "mi2"),
    // speed — base: metre per second
    ("m/s", Dim::Speed, 1.0, "m/s"),
    ("mps", Dim::Speed, 1.0, "m/s"),
    ("km/h", Dim::Speed, 0.27777777777777779, "km/h"),
    ("kph", Dim::Speed, 0.27777777777777779, "km/h"),
    ("kmph", Dim::Speed, 0.27777777777777779, "km/h"),
    ("mph", Dim::Speed, 0.44704, "mph"),
    ("mi/h", Dim::Speed, 0.44704, "mph"),
    ("kn", Dim::Speed, 0.51444444444444448, "kn"),
    ("knot", Dim::Speed, 0.51444444444444448, "kn"),
    ("knots", Dim::Speed, 0.51444444444444448, "kn"),
    ("ft/s", Dim::Speed, 0.3048, "ft/s"),
    // data — base: byte, decimal by default
    ("b", Dim::Data, 1.0, "b"),
    ("byte", Dim::Data, 1.0, "b"),
    ("bytes", Dim::Data, 1.0, "b"),
    ("kb", Dim::Data, 1000.0, "kb"),
    ("kilobyte", Dim::Data, 1000.0, "kb"),
    ("kilobytes", Dim::Data, 1000.0, "kb"),
    ("mb", Dim::Data, 1000000.0, "mb"),
    ("megabyte", Dim::Data, 1000000.0, "mb"),
    ("megabytes", Dim::Data, 1000000.0, "mb"),
    ("gb", Dim::Data, 1000000000.0, "gb"),
    ("gigabyte", Dim::Data, 1000000000.0, "gb"),
    ("gigabytes", Dim::Data, 1000000000.0, "gb"),
    ("tb", Dim::Data, 1000000000000.0, "tb"),
    ("terabyte", Dim::Data, 1000000000000.0, "tb"),
    ("terabytes", Dim::Data, 1000000000000.0, "tb"),
    ("kib", Dim::Data, 1024.0, "kib"),
    ("kibibyte", Dim::Data, 1024.0, "kib"),
    ("kibibytes", Dim::Data, 1024.0, "kib"),
    ("mib", Dim::Data, 1048576.0, "mib"),
    ("mebibyte", Dim::Data, 1048576.0, "mib"),
    ("mebibytes", Dim::Data, 1048576.0, "mib"),
    ("gib", Dim::Data, 1073741824.0, "gib"),
    ("gibibyte", Dim::Data, 1073741824.0, "gib"),
    ("gibibytes", Dim::Data, 1073741824.0, "gib"),
    ("tib", Dim::Data, 1099511627776.0, "tib"),
    ("tebibyte", Dim::Data, 1099511627776.0, "tib"),
    ("tebibytes", Dim::Data, 1099511627776.0, "tib"),
    // pressure — base: pascal
    ("pa", Dim::Press, 1.0, "pa"),
    ("pascal", Dim::Press, 1.0, "pa"),
    ("pascals", Dim::Press, 1.0, "pa"),
    ("kpa", Dim::Press, 1000.0, "kpa"),
    ("mpa", Dim::Press, 1000000.0, "mpa"),
    ("bar", Dim::Press, 100000.0, "bar"),
    ("mbar", Dim::Press, 100.0, "mbar"),
    ("atm", Dim::Press, 101325.0, "atm"),
    ("psi", Dim::Press, 6894.757293168, "psi"),
    ("mmhg", Dim::Press, 133.322387415, "mmhg"),
    ("torr", Dim::Press, 133.322368421, "torr"),
    // energy — base: joule
    ("j", Dim::Energy, 1.0, "j"),
    ("joule", Dim::Energy, 1.0, "j"),
    ("joules", Dim::Energy, 1.0, "j"),
    ("kj", Dim::Energy, 1000.0, "kj"),
    ("mj", Dim::Energy, 1000000.0, "mj"),
    ("cal", Dim::Energy, 4.184, "cal"),
    ("calorie", Dim::Energy, 4.184, "cal"),
    ("calories", Dim::Energy, 4.184, "cal"),
    ("kcal", Dim::Energy, 4184.0, "kcal"),
    ("kilocalorie", Dim::Energy, 4184.0, "kcal"),
    ("kilocalories", Dim::Energy, 4184.0, "kcal"),
    ("wh", Dim::Energy, 3600.0, "wh"),
    ("kwh", Dim::Energy, 3600000.0, "kwh"),
    ("btu", Dim::Energy, 1055.05585262, "btu"),
    // temperature — affine, factor unused, code picks the formula
    ("c", Dim::Temp, 1.0, "c"),
    ("celsius", Dim::Temp, 1.0, "c"),
    ("centigrade", Dim::Temp, 1.0, "c"),
    ("f", Dim::Temp, 1.0, "f"),
    ("fahrenheit", Dim::Temp, 1.0, "f"),
    ("k", Dim::Temp, 1.0, "k"),
    ("kelvin", Dim::Temp, 1.0, "k"),
];

/// The longest alias in [`UNITS`]; bounds the suffix scan in [`parse_amount`].
const MAX_ALIAS: usize = 12;

/// Resolve a unit name (already lowercase) to its dimension and factor.
fn unit_of(name: &str) -> Option<Unit> {
    let n = name.trim();
    UNITS
        .iter()
        .find(|(alias, _, _, _)| *alias == n)
        .map(|(_, dim, factor, code)| Unit { dim: *dim, factor: *factor, code: *code })
}

/// The numeric value on the left of a conversion: a literal, or an arithmetic
/// expression the calc engine already understands (`2*3`, `(1+1)/2`).
fn left_value(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    calc(t).ok()
}

/// Parse the amount side of a conversion: `10 km`, `5kg`, `1e3 m`, `2*3 km`.
///
/// The unit is found by *resolving* a candidate against [`UNITS`], never by
/// cutting at a fixed column, which is what makes `10 in in cm` read as ten
/// inches into centimetres.
fn parse_amount(s: &str) -> Option<(f64, Unit)> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    // "10 km" / "1e3 m" / "2*3 km" — unit is the last whitespace-separated token.
    if let Some((head, tail)) = t.rsplit_once(char::is_whitespace) {
        if let (Some(u), Some(v)) = (unit_of(tail), left_value(head)) {
            return Some((v, u));
        }
    }
    // "5kg" / "1.5kg" / "10km" — shortest suffix that is a known unit *and*
    // leaves a number (or an expression) in front of it.
    let len = t.len();
    for k in 1..=MAX_ALIAS.min(len) {
        let i = len - k;
        if !t.is_char_boundary(i) {
            continue;
        }
        let (head, tail) = t.split_at(i);
        if let (Some(u), Some(v)) = (unit_of(tail), left_value(head)) {
            return Some((v, u));
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

/// Words that turn one amount into another: `10 km in mi`, `2 h to min`.
const CONNECTORS: &[&str] = &[" in ", " to ", " as ", " -> ", " => ", " = "];

/// Convert a natural-language-ish expression such as `10 km in mi`,
/// `2 h to min`, `10 GB in MB`. Returns `None` when it is not a conversion.
///
/// Candidate splits are tried right to left and the first one that resolves on
/// both sides wins. That ordering is deliberate: the unit itself may be a
/// connector word (`in` is inch), so the split is decided by what parses, not
/// by where the first `" in "` happens to sit.
pub fn convert_units(s: &str) -> Option<String> {
    let low = s.trim().to_lowercase();

    let mut splits: Vec<(usize, usize)> = Vec::new();
    for c in CONNECTORS {
        let mut from = 0;
        while let Some(off) = low[from..].find(c) {
            let at = from + off;
            splits.push((at, c.len()));
            from = at + 1;
        }
    }
    splits.sort_by(|a, b| b.0.cmp(&a.0));

    for (at, clen) in splits {
        let (left, right) = (&low[..at], &low[at + clen..]);
        let Some(to) = right.split_whitespace().next().and_then(unit_of) else {
            continue;
        };
        let Some((value, from)) = parse_amount(left) else {
            continue;
        };
        if from.dim != to.dim {
            continue;
        }
        let out = if from.dim == Dim::Temp {
            from_celsius(to_celsius(value, from.code), to.code)
        } else {
            value * from.factor / to.factor
        };
        return Some(format!("{s} = {}", fmt_num(out)));
    }
    None
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
    check!(
        "convert h->min",
        convert_units("2 h in min").map(|r| r.ends_with("= 120")).unwrap_or(false)
    );
    check!(
        "convert gb->mb",
        convert_units("10 gb in mb").map(|r| r.ends_with("= 10000")).unwrap_or(false)
    );
    check!(
        "convert inch vs connector",
        convert_units("10 in in cm").map(|r| r.ends_with("= 25.4")).unwrap_or(false)
    );
    check!(
        "convert exponent literal",
        convert_units("1e3 m in km").map(|r| r.ends_with("= 1")).unwrap_or(false)
    );
    check!("convert non-conversion is None", convert_units("hello world").is_none());

    if fails == 0 {
        println!("self-test: OK (18/18)");
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
    fn conversion_time_volume_data() {
        assert_eq!(convert_units("2 h in min").as_deref(), Some("2 h in min = 120"));
        assert_eq!(convert_units("90 min in h").as_deref(), Some("90 min in h = 1.5"));
        assert_eq!(convert_units("10 gb in mb").as_deref(), Some("10 gb in mb = 10000"));
        assert_eq!(convert_units("1 gib in mib").as_deref(), Some("1 gib in mib = 1024"));
        assert!(convert_units("3 tbsp in ml").unwrap().contains("44.3602"));
    }

    #[test]
    fn conversion_speed_area_pressure_energy() {
        assert!(convert_units("5 km/h in mph").unwrap().contains("3.1068"));
        assert!(convert_units("1 acre in m2").unwrap().contains("4046.85"));
        assert!(convert_units("1 bar in psi").unwrap().contains("14.5037"));
        assert!(convert_units("1 kcal in j").unwrap().contains("4184"));
    }

    #[test]
    fn conversion_parsing_resolves_the_split() {
        // "in" is both a connector and the inch: the split is decided by what
        // parses, not by where the first " in " happens to sit.
        assert!(convert_units("10 in in cm").unwrap().contains("25.4"));
        // Exponents and expressions on the amount side.
        assert_eq!(convert_units("1e3 m in km").as_deref(), Some("1e3 m in km = 1"));
        assert!(convert_units("2*3 km in mi").unwrap().contains("3.7282"));
        // One dimension cannot become another.
        assert!(convert_units("10 km in kg").is_none());
        assert!(convert_units("2 h in gb").is_none());
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
