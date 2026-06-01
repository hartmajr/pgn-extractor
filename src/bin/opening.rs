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

use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::process;

use shakmaty::fen::Fen;
use shakmaty::san::San;
use shakmaty::zobrist::{Zobrist64, ZobristHash};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position};

use zstd::stream::read::Decoder;

use chess_extract::{tag_value, take_flag, take_min_tc, tc_passes, DEFAULT_MIN_TC};

fn extract_moves(text: &[u8], limit: usize) -> Vec<&[u8]> {
    let mut out: Vec<&[u8]> = Vec::new();
    let n = text.len();
    let mut i = 0;
    while i < n && out.len() < limit {
        let c = text[i];
        if c.is_ascii_whitespace() { i += 1; continue; }
        if c == b'{' { while i < n && text[i] != b'}' { i += 1; } i += 1; continue; }
        if c == b';' { while i < n && text[i] != b'\n' { i += 1; } continue; }
        if c == b'(' {
            let mut depth = 1; i += 1;
            while i < n && depth > 0 {
                match text[i] { b'(' => depth += 1, b')' => depth -= 1, _ => {} }
                i += 1;
            }
            continue;
        }
        if c == b')' || c == b'}' { i += 1; continue; }
        let start = i;
        while i < n && !text[i].is_ascii_whitespace() { i += 1; }
        let mut tok = &text[start..i];
        if is_result(tok) { break; }
        if tok[0] == b'$' || is_move_number(tok) { continue; }
        let mut end = tok.len();
        while end > 0 {
            match tok[end - 1] { b'+' | b'#' | b'!' | b'?' => end -= 1, _ => break }
        }
        tok = &tok[..end];
        if !tok.is_empty() { out.push(tok); }
    }
    out
}

fn is_result(tok: &[u8]) -> bool {
    matches!(tok, b"1-0" | b"0-1" | b"1/2-1/2" | b"*")
}

fn is_move_number(tok: &[u8]) -> bool {
    tok.iter().all(|&b| b.is_ascii_digit() || b == b'.')
}

/// Zobrist key of the position reached by playing a SAN move sequence from the start.
fn moves_key(moves: &[&[u8]]) -> Option<Zobrist64> {
    let mut pos = Chess::default();
    for mv in moves {
        let san = San::from_ascii(mv).ok()?;
        let m = san.to_move(&pos).ok()?;
        pos = pos.play(&m).ok()?;
    }
    Some(pos.zobrist_hash(EnPassantMode::Legal))
}

/// Zobrist key of a position given as a FEN. Move counters are optional and ignored.
fn fen_key(line: &str) -> Option<Zobrist64> {
    let mut fields: Vec<&str> = line.split_whitespace().collect();
    while fields.len() < 6 {
        fields.push(if fields.len() == 4 { "0" } else { "1" });
    }
    let normalized = fields.join(" ");
    let pos: Chess = normalized
        .parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()?;
    Some(pos.zobrist_hash(EnPassantMode::Legal))
}

/// Does this game pass through any target position within the replayed plies?
fn matches_target(moves: &[&[u8]], targets: &HashSet<Zobrist64>) -> bool {
    let mut pos = Chess::default();
    for mv in moves {
        let san = match San::from_ascii(mv) { Ok(s) => s, Err(_) => return false };
        let m = match san.to_move(&pos) { Ok(m) => m, Err(_) => return false };
        pos = match pos.play(&m) { Ok(p) => p, Err(_) => return false };
        let key: Zobrist64 = pos.zobrist_hash(EnPassantMode::Legal);
        if targets.contains(&key) { return true; }
    }
    false
}

struct Targets {
    keys: HashSet<Zobrist64>,
    move_depth: usize, // longest move-line, in plies
    has_fen: bool,
}

fn load_targets(path: &str) -> io::Result<Targets> {
    let mut content = String::new();
    File::open(path)?.read_to_string(&mut content)?;
    let mut keys = HashSet::new();
    let mut move_depth = 0;
    let mut has_fen = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        // A FEN contains rank separators; a move line never does.
        let key = if line.contains('/') {
            has_fen = true;
            fen_key(line)
        } else {
            let moves = extract_moves(line.as_bytes(), usize::MAX);
            move_depth = move_depth.max(moves.len());
            moves_key(&moves)
        };
        match key {
            Some(k) => { keys.insert(k); }
            None => eprintln!("Warning: could not parse target: {line}"),
        }
    }
    Ok(Targets { keys, move_depth, has_fen })
}

fn main() -> io::Result<()> {
    let mut args: Vec<String> = env::args().collect();
    let min_tc = take_min_tc(&mut args, DEFAULT_MIN_TC);
    let allow_correspondence = !take_flag(&mut args, "--no-correspondence");
    if args.len() < 4 {
        eprintln!("Usage: {} <input.pgn.zst> <output.pgn> <targets.txt> [max_ply] [--min-tc <sec>]",
            args.get(0).map(String::as_str).unwrap_or("opening-filter"));
        eprintln!("  targets.txt: each line is a move sequence OR a FEN.");
        eprintln!("    move line: e4 e5 Nf3 Nc6 Nc3 Nf6 d3");
        eprintln!("    FEN:       r1bqkb1r/pppp1ppp/2n2n2/4p3/4P3/2NP1N2/PPP2PPP/R1BQKB1R b KQkq - 0 1");
        eprintln!("  max_ply: optional cap on how many plies of each game to search.");
        eprintln!("    Defaults to the opening depth for move-only files, or the whole game when");
        eprintln!("    FENs are present. Set it (e.g. 30) to bound the search and run faster.");
        eprintln!("  --min-tc: minimum time control in seconds (default {DEFAULT_MIN_TC}; removes");
        eprintln!("    bullet/ultrabullet). Set 0 to keep all time controls.");
        eprintln!("  --no-correspondence also drops correspondence games (TimeControl \"-\").");
        process::exit(2);
    }
    let (input_path, output_path, targets_path) = (&args[1], &args[2], &args[3]);
    let max_ply_arg: Option<usize> = args.get(4).and_then(|s| s.parse().ok());

    let targets = load_targets(targets_path)?;
    if targets.keys.is_empty() {
        eprintln!("No usable targets found in {targets_path}");
        process::exit(1);
    }

    // How far into each game we need to replay.
    let limit = if targets.has_fen {
        match max_ply_arg {
            Some(p) => p.max(targets.move_depth),
            None => usize::MAX,
        }
    } else {
        max_ply_arg.unwrap_or(targets.move_depth)
    };
    let limit_desc = if limit == usize::MAX { "whole game".to_string() } else { format!("{limit} plies") };
    eprintln!("Loaded {} distinct target position(s). Searching {} of each game; time control >= {min_tc}s.",
        targets.keys.len(), limit_desc);
    if !allow_correspondence { eprintln!("Excluding correspondence games."); }

    let decoder = Decoder::new(File::open(input_path)?)?;
    let mut reader = BufReader::with_capacity(16 * 1024 * 1024, decoder);
    let mut writer = BufWriter::with_capacity(16 * 1024 * 1024, File::create(output_path)?);

    let mut game: Vec<u8> = Vec::with_capacity(8192);
    let mut movetext: Vec<u8> = Vec::with_capacity(4096);
    let mut in_movetext = false;
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
            let moves = extract_moves(&movetext, limit);
            if tc_ok && matches_target(&moves, &targets.keys) {
                writer.write_all(&game)?;
                kept += 1;
            }
            game.clear();
            movetext.clear();
            in_movetext = false;
            tc_ok = true;
            if total % 2_000_000 == 0 {
                eprintln!("  processed {total} games, kept {kept} ...");
            }
        }

        if eof { break; }

        if !in_movetext && line.starts_with(b"[TimeControl ") {
            if let Some(v) = tag_value(&line) {
                tc_ok = tc_passes(v, min_tc, allow_correspondence);
            }
        }

        if !in_movetext && (line == b"\n" || line == b"\r\n") {
            in_movetext = true;
        } else if in_movetext {
            movetext.extend_from_slice(&line);
        }
        game.extend_from_slice(&line);
    }

    writer.flush()?;
    eprintln!("Done. Kept {kept} of {total} games.");
    Ok(())
}
