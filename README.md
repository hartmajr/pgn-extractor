# chess-extract

Fast Rust command-line tools for filtering the [Lichess open database](https://database.lichess.org/)
of monthly games.

The Lichess "standard rated" monthly dumps are distributed as a single
Zstandard-compressed PGN file (`.pgn.zst`) — roughly 30 GB compressed, expanding
about 7× to 200+ GB and up to ~100 million games. These tools stream that file
**directly** (no need to decompress it first), filter it, and copy every matching
game out **verbatim** — clocks, evaluations and comments preserved byte-for-byte.

There are three independent executables, each for one job:

| Tool      | Filters by                                   |
|-----------|----------------------------------------------|
| `elite`   | player ratings                               |
| `eco`     | ECO opening code                             |
| `opening` | opening line or position (FEN), incl. transpositions |

All three additionally apply a minimum time-control floor (default 180 s, removing
bullet and ultrabullet); see [Time control](#time-control--shared-by-all-three-tools).
They share no state and are run separately. `cargo build --release` builds all three.

---

## Building

You need the Rust toolchain. Pick whichever path suits you.

### Option 1 — Let GitHub build the Windows `.exe` (no local setup)

Push this project to a GitHub repo. The included workflow
(`.github/workflows/build-windows.yml`) compiles on a Windows runner and uploads
`elite.exe`, `eco.exe` and `opening.exe` as a downloadable artifact under the
**Actions** tab. The runners already have Rust and the MSVC build tools, and the
workflow statically links the C runtime, so the binaries are standalone (no extra
DLLs to ship). You can also trigger it manually via **Run workflow**.

### Option 2 — Compile natively on Windows

1. Install Rust from <https://rustup.rs> and let it set up the **MSVC** build tools
   (or install "Visual Studio Build Tools" with the *Desktop development with C++*
   workload). This C toolchain is also what compiles the bundled zstd library.
2. From the project folder: `cargo build --release`
3. Binaries land in `target\release\elite.exe`, `eco.exe`, `opening.exe`.

### Option 3 — Cross-compile from Linux / WSL

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64          # or your distro's equivalent
cargo build --release --target x86_64-pc-windows-gnu
```

> The chess engine (`shakmaty`) is pure Rust and adds no toolchain requirements.
> Only the `zstd` crate compiles C code, which the MSVC tools / runners / mingw
> above provide.

---

## Usage

All three read a `.pgn.zst` file directly and write a plain `.pgn` of the matches.
Progress and a final count are printed to stderr.

### `elite` — ratings and time control

```
elite <input.pgn.zst> <output.pgn> [min_low=2400] [min_high=min_low] [--min-tc <sec>]
```

Keeps a game when the **lower-rated** player is at least `min_low` and the
**higher-rated** player is at least `min_high`. Colour is irrelevant — the floors are
applied to the lower and higher of the two ratings. `min_high` defaults to `min_low`,
which gives the simple "both players ≥ X" case; set it higher for an asymmetric pairing.

```sh
# Both players >= 2400, no bullet/ultrabullet (the defaults)
elite lichess_db_standard_rated_2025-05.pgn.zst elite_2025-05.pgn 2400

# One player >= 2500, the other >= 2300
elite lichess_db_standard_rated_2025-05.pgn.zst mixed_2025-05.pgn 2300 2500

# Both players >= 2200, rapid or slower (>= 8 min estimated)
elite lichess_db_standard_rated_2025-05.pgn.zst strong_2025-05.pgn 2200 --min-tc 480
```

### `eco` — opening code

```
eco <input.pgn.zst> <output.pgn> <code-prefix> [more-prefixes...] [--min-tc <sec>]
```

Keeps a game whose `ECO` tag **starts with** any of the given prefixes (and which
clears the time-control floor). Because it's a prefix match, you can give an exact
code or a whole family:

```sh
eco month.pgn.zst najdorf.pgn B90              # exactly B90
eco month.pgn.zst najdorf_all.pgn B9           # B90–B99
eco month.pgn.zst all_sicilians.pgn B2 B3 B4 B5 B6 B7 B8 B9   # B20–B99
eco month.pgn.zst all_B.pgn B                  # every B code
```

### `opening` — opening line or position

```
opening <input.pgn.zst> <output.pgn> <targets.txt> [max_ply] [--min-tc <sec>]
```

Matches by the **position reached**, not by move text, so transpositions are caught
automatically — you don't have to enumerate move orders. Each non-comment line in
`targets.txt` is either a move sequence or a FEN (auto-detected by the `/` in a FEN):

```
# targets.txt — one position per line; lines starting with # are ignored.
# Move sequences (one move order per position is enough):
e4 e5 Nf3 Nc6 Nc3 Nf6 d3
e4 e5 Nf3 Nc6 Nc3 Nf6 Bb5
# FENs (move counters are optional and ignored):
r1bqkb1r/pppp1ppp/2n2n2/4p3/4P3/2NP1N2/PPP2PPP/R1BQKB1R b KQkq - 0 1
```

```sh
opening month.pgn.zst vienna.pgn targets.txt
opening month.pgn.zst vienna.pgn targets.txt 30   # only search first 30 plies
```

Notes:

- **Transpositions** into a target position are matched regardless of move order, so
  a single move order (or a single FEN) per position suffices.
- **FENs** match on piece placement, side to move, castling rights and en passant —
  the move-counter fields can be anything. Because castling rights are part of the
  position, a FEN with `KQkq` will not match a game that reached the same piece
  placement after already forfeiting a castling right (this is intentional).
- **`max_ply`** bounds how deep into each game the search goes. For opening lines the
  tool only searches as deep as your longest line. For FENs, which can occur anywhere,
  it searches the whole game by default; cap it (e.g. `30`) when you know the position
  is reached early and want it to run faster.

### Time control — shared by all three tools

Every tool applies a minimum time-control floor, so bullet and ultrabullet games are
removed from all output by default. The control is measured by Lichess's own estimated
game duration, `base + 40 × increment` seconds; the default floor is **180 seconds**,
which removes bullet and ultrabullet while keeping blitz and slower. Correspondence
games (no clock) are kept.

Override it per run with `--min-tc <seconds>`, which may appear anywhere on the command
line:

```sh
elite   month.pgn.zst out.pgn 2400 --min-tc 480   # rapid or slower only
opening month.pgn.zst out.pgn targets.txt --min-tc 0   # keep every time control
```

---

## Performance

| Workload                         | Cost                                            |
|----------------------------------|-------------------------------------------------|
| `elite`, `eco`                   | tag-only; decompression-bound, ~minutes/month   |
| `opening` with move lines        | replays only the opening; ~decompression-bound  |
| `opening` with FENs (deep)       | may replay whole games; CPU-bound, tens of min  |

Decompression of a single `.zst` stream runs on one core and sets the floor for the
tag filters. The FEN case is the one genuinely CPU-bound workload — to speed it up,
cap `max_ply`, or process several monthly files at once (one core each).

---

## License

This project is licensed under the **GNU General Public License v3.0 or later**
(GPL-3.0-or-later). See the [`LICENSE`](LICENSE) file for the full text.

It links the [`shakmaty`](https://crates.io/crates/shakmaty) chess library, which is
GPL-3.0-or-later, so copyleft applies to distributed binaries regardless. The `zstd`
crates are permissive (MIT / Apache-2.0) and compatible with GPL-3.0. None of the
original pgn-extract source is used; this is an independent implementation.

Copyright © 2026 John Hartmann.
