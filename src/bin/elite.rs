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

use chess_extract::{parse_u32, tag_value, take_min_tc, tc_passes, DEFAULT_MIN_TC};

fn main() -> io::Result<()> {
    let mut args: Vec<String> = env::args().collect();
    let min_tc = take_min_tc(&mut args, DEFAULT_MIN_TC);

    if args.len() < 3 {
        eprintln!("Usage: {} <input.pgn.zst> <output.pgn> [min_low=2400] [min_high=min_low] [--min-tc <sec>]",
            args.get(0).map(String::as_str).unwrap_or("elite"));
        eprintln!("  Keeps a game when the LOWER-rated player is >= min_low AND the HIGHER-rated");
        eprintln!("  player is >= min_high, and the time control is >= --min-tc seconds.");
        eprintln!("  Colour is irrelevant. min_high defaults to min_low (i.e. both players >= min_low).");
        eprintln!("  --min-tc defaults to {DEFAULT_MIN_TC} (removes bullet/ultrabullet); set 0 to keep all.");
        eprintln!("  Examples:");
        eprintln!("    elite in.zst out.pgn 2400                  # both players >= 2400");
        eprintln!("    elite in.zst out.pgn 2300 2500             # one >= 2500, the other >= 2300");
        eprintln!("    elite in.zst out.pgn 2400 --min-tc 480     # both >= 2400, rapid or slower");
        process::exit(2);
    }
    let input_path = &args[1];
    let output_path = &args[2];
    let min_low: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2400);
    let min_high: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(min_low);

    eprintln!("Lower-rated player >= {min_low}, higher-rated player >= {min_high}, time control >= {min_tc}s");

    let decoder = Decoder::new(File::open(input_path)?)?;
    let mut reader = BufReader::with_capacity(16 * 1024 * 1024, decoder);
    let mut writer = BufWriter::with_capacity(16 * 1024 * 1024, File::create(output_path)?);

    let mut game: Vec<u8> = Vec::with_capacity(8192);
    let mut white_elo = 0u32;
    let mut black_elo = 0u32;
    let mut tc_ok = true;
    let mut total: u64 = 0;
    let mut kept: u64 = 0;
    let mut line: Vec<u8> = Vec::with_capacity(256);

    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        let eof = n == 0;

        if (eof || line.starts_with(b"[Event ")) && !game.is_empty() {
            total += 1;
            let lo = white_elo.min(black_elo);
            let hi = white_elo.max(black_elo);
            if lo >= min_low && hi >= min_high && tc_ok {
                writer.write_all(&game)?;
                kept += 1;
            }
            game.clear();
            white_elo = 0;
            black_elo = 0;
            tc_ok = true;
            if total % 2_000_000 == 0 {
                eprintln!("  processed {total} games, kept {kept} ...");
            }
        }
        if eof { break; }

        if line.starts_with(b"[WhiteElo ") {
            white_elo = tag_value(&line).and_then(parse_u32).unwrap_or(0);
        } else if line.starts_with(b"[BlackElo ") {
            black_elo = tag_value(&line).and_then(parse_u32).unwrap_or(0);
        } else if line.starts_with(b"[TimeControl ") {
            if let Some(v) = tag_value(&line) {
                tc_ok = tc_passes(v, min_tc);
            }
        }
        game.extend_from_slice(&line);
    }
    writer.flush()?;
    eprintln!("Done. Kept {kept} of {total} games.");
    Ok(())
}
