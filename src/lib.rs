// chess-extract: fast filters for the Lichess game database.
// Copyright (C) 2026  John Hartmann
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Shared helpers used by the elite, eco and opening filters.

/// Default minimum time control, in seconds. 180 removes bullet and ultrabullet.
pub const DEFAULT_MIN_TC: u32 = 180;

/// Extract the quoted value from a PGN tag line, e.g. `[ECO "B90"]` -> b"B90".
pub fn tag_value(line: &[u8]) -> Option<&[u8]> {
    let start = line.iter().position(|&b| b == b'"')? + 1;
    let rel_end = line[start..].iter().position(|&b| b == b'"')?;
    Some(&line[start..start + rel_end])
}

pub fn parse_u32(bytes: &[u8]) -> Option<u32> {
    std::str::from_utf8(bytes).ok()?.trim().parse().ok()
}

/// Lichess estimated game duration: base + 40 * increment, in seconds.
/// None for correspondence ("-") or anything unparseable.
pub fn tc_estimated_seconds(tc: &[u8]) -> Option<u32> {
    if tc == b"-" { return None; }
    let s = std::str::from_utf8(tc).ok()?;
    let mut parts = s.split('+');
    let base: u32 = parts.next()?.trim().parse().ok()?;
    let inc: u32 = parts.next().unwrap_or("0").trim().parse().ok()?;
    Some(base + 40 * inc)
}

/// Does a TimeControl tag value clear the floor?
///
/// - Correspondence games (`"-"`) are kept iff `allow_correspondence` is true.
/// - Real clock controls are kept iff their estimated duration is >= `min_tc`.
/// - Anything else unparseable is treated as passing (we can't measure it, so we
///   don't drop it).
pub fn tc_passes(tc: &[u8], min_tc: u32, allow_correspondence: bool) -> bool {
    if tc == b"-" {
        return allow_correspondence;
    }
    match tc_estimated_seconds(tc) {
        Some(secs) => secs >= min_tc,
        None => true,
    }
}

/// Pull an optional boolean flag (e.g. `--no-correspondence`) out of `args`,
/// removing every occurrence in place. Returns true if the flag was present.
pub fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    let mut found = false;
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            args.remove(i);
            found = true;
        } else {
            i += 1;
        }
    }
    found
}

/// Pull an optional `--min-tc <seconds>` / `--min-tc=<seconds>` flag out of `args`,
/// removing it in place so the remaining positional arguments are unaffected.
/// Returns the chosen floor, or `default` if the flag is absent (or its value
/// doesn't parse, in which case a warning is printed and the default is kept —
/// the safe direction, since the default never lets bullet through).
pub fn take_min_tc(args: &mut Vec<String>, default: u32) -> u32 {
    let mut value: Option<u32> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(rest) = a.strip_prefix("--min-tc=") {
            match rest.parse() {
                Ok(v) => value = Some(v),
                Err(_) => eprintln!("Warning: invalid --min-tc value '{rest}', using {default}"),
            }
            args.remove(i);
        } else if a == "--min-tc" {
            args.remove(i); // remove flag
            if i < args.len() {
                let v = args.remove(i); // and consume its value
                match v.parse() {
                    Ok(v) => value = Some(v),
                    Err(_) => eprintln!("Warning: invalid --min-tc value '{v}', using {default}"),
                }
            } else {
                eprintln!("Warning: --min-tc given with no value, using {default}");
            }
        } else {
            i += 1;
        }
    }
    value.unwrap_or(default)
}
