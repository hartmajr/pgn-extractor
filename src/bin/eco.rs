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

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::process;

use zstd::stream::read::Decoder;

fn tag_value(line: &[u8]) -> Option<&[u8]> {
    let start = line.iter().position(|&b| b == b'"')? + 1;
    let rel_end = line[start..].iter().position(|&b| b == b'"')?;
    Some(&line[start..start + rel_end])
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: {} <input.pgn.zst> <output.pgn> <code-prefix> [more-prefixes...]",
            args.get(0).map(String::as_str).unwrap_or("eco"));
        eprintln!("  Keeps games whose ECO tag starts with any given prefix.");
        eprintln!("  Examples: `B90` (exact), `B9` (B90-B99), `B` (all B), `B2 B3 B4` (several).");
        process::exit(2);
    }
    let input_path = &args[1];
    let output_path = &args[2];
    let prefixes: Vec<Vec<u8>> = args[3..].iter().map(|s| s.as_bytes().to_vec()).collect();

    eprintln!("Matching ECO prefixes: {}", args[3..].join(", "));

    let decoder = Decoder::new(File::open(input_path)?)?;
    let mut reader = BufReader::with_capacity(16 * 1024 * 1024, decoder);
    let mut writer = BufWriter::with_capacity(16 * 1024 * 1024, File::create(output_path)?);

    let mut game: Vec<u8> = Vec::with_capacity(8192);
    let mut eco_match = false;
    let mut total: u64 = 0;
    let mut kept: u64 = 0;
    let mut line: Vec<u8> = Vec::with_capacity(256);

    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        let eof = n == 0;

        if (eof || line.starts_with(b"[Event ")) && !game.is_empty() {
            total += 1;
            if eco_match {
                writer.write_all(&game)?;
                kept += 1;
            }
            game.clear();
            eco_match = false;
            if total % 2_000_000 == 0 {
                eprintln!("  processed {total} games, kept {kept} ...");
            }
        }
        if eof { break; }

        if line.starts_with(b"[ECO ") {
            if let Some(v) = tag_value(&line) {
                eco_match = prefixes.iter().any(|p| v.starts_with(p.as_slice()));
            }
        }
        game.extend_from_slice(&line);
    }
    writer.flush()?;
    eprintln!("Done. Kept {kept} of {total} games.");
    Ok(())
}
