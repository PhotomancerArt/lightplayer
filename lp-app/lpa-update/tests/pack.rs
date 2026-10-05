//! The one packer (feature `pack`): encoding 1's `.z` stream and index,
//! every chunk proven through the board's own decoder.

#![cfg(feature = "pack")]

use lpa_update::EncodedPiece;
use lpa_update::pack::{ProveError, pack_piece, prove_piece};
use lpc_update::PieceKind;
use lpc_update::code_table::CHUNK;

fn rng(seed: u32) -> impl FnMut() -> u32 {
    let mut s = seed | 1;
    move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    }
}

fn random(len: usize, seed: u32) -> Vec<u8> {
    let mut next = rng(seed);
    (0..len).map(|_| next() as u8).collect()
}

/// Firmware-like: a 10 KiB block repeated with sparse changes.
fn repeating(len: usize, seed: u32) -> Vec<u8> {
    let mut next = rng(seed);
    let block: Vec<u8> = (0..10 * 1024).map(|_| (next() % 11) as u8).collect();
    (0..len)
        .map(|i| {
            if i % 89 == 0 {
                next() as u8
            } else {
                block[i % block.len()]
            }
        })
        .collect()
}

fn invariants(piece: &[u8], e: &EncodedPiece) {
    assert_eq!(e.chunks.len(), piece.len().div_ceil(CHUNK as usize));
    assert_eq!(
        e.chunks.iter().map(|&c| c as usize).sum::<usize>(),
        e.stream.len()
    );
    e.check(piece.len()).unwrap();
}

#[test]
fn every_kind_of_input_round_trips_through_lp_deflate() {
    for kind in [PieceKind::Core, PieceKind::Engine] {
        for piece in [
            random(5 * 4096 + 123, 1),
            vec![0u8; 7 * 4096],
            repeating(9 * 4096 + 7, 2),
            vec![0u8; 100],
        ] {
            let e = pack_piece(kind, &piece);
            invariants(&piece, &e);
            prove_piece(kind, &piece, &e).unwrap();
        }
    }
}

#[test]
fn incompressible_data_has_an_all_zero_index_and_an_empty_stream() {
    let piece = random(6 * 4096, 3);
    let e = pack_piece(PieceKind::Core, &piece);
    assert!(e.chunks.iter().all(|&c| c == 0));
    assert!(e.stream.is_empty());
    assert!((e.ratio(piece.len()) - 1.0).abs() < 1e-9);
}

#[test]
fn a_two_megabyte_piece_with_repeats_packs_small_and_proves() {
    let piece = repeating(2 * 1024 * 1024 + 333, 4);
    let e = pack_piece(PieceKind::Engine, &piece);
    invariants(&piece, &e);
    prove_piece(PieceKind::Engine, &piece, &e).unwrap();
    let ratio = e.ratio(piece.len());
    println!("2 MiB synthetic engine: {:.1}% of raw", ratio * 100.0);
    assert!(ratio < 0.5, "{ratio}");
}

#[test]
fn the_same_piece_gives_the_same_bytes() {
    let piece = repeating(300 * 1024, 5);
    assert_eq!(
        pack_piece(PieceKind::Core, &piece),
        pack_piece(PieceKind::Core, &piece)
    );
}

#[test]
fn the_dictionary_is_applied() {
    // A random block that repeats every 10 KiB: a chunk whose bytes are
    // inside the previous 32 KiB compresses to almost nothing with its
    // dictionary, and not at all without one.
    let block = random(10 * 1024, 6);
    let piece: Vec<u8> = block.iter().copied().cycle().take(64 * 1024).collect();
    let with = pack_piece(PieceKind::Core, &piece);
    // Chunk 0 has no dictionary (random: no compressed form); chunk 4
    // (16 KiB) lies wholly inside what came before.
    assert_eq!(with.chunks[0], 0);
    assert!(
        with.chunks[4] > 0 && with.chunks[4] < 200,
        "{:?}",
        with.chunks
    );
    // The same chunk as an engine's chunk 1 has no dictionary: incompressible.
    let alone = pack_piece(PieceKind::Engine, &piece[16 * 1024 - 4096..]);
    assert_eq!(alone.chunks[1], 0);
}

#[test]
fn prove_piece_catches_a_flipped_byte_and_a_wrong_index() {
    let piece = repeating(8 * 4096, 7);
    let e = pack_piece(PieceKind::Core, &piece);
    let first = e.chunks.iter().position(|&c| c > 0).unwrap();
    let mut flipped = e.clone();
    let at: usize = e.chunks[..first].iter().map(|&c| c as usize).sum::<usize>() + 2;
    flipped.stream[at] ^= 0x10;
    assert_eq!(
        prove_piece(PieceKind::Core, &piece, &flipped),
        Err(ProveError::Chunk { idx: first as u32 })
    );
    let mut short = e.clone();
    short.chunks.pop();
    assert_eq!(
        prove_piece(PieceKind::Core, &piece, &short),
        Err(ProveError::Shape)
    );
    // Lengths swapped between two compressed chunks: the sum still holds,
    // the chunks do not.
    let mut swapped = e.clone();
    let nz: Vec<usize> = (0..e.chunks.len()).filter(|&i| e.chunks[i] > 0).collect();
    if let [a, b, ..] = nz[..]
        && e.chunks[a] != e.chunks[b]
    {
        swapped.chunks.swap(a, b);
        assert!(prove_piece(PieceKind::Core, &piece, &swapped).is_err());
    }
}

/// A real `engine.bin`, when one is at hand (`LPA_UPDATE_ENGINE_BIN`):
/// the ratio for the report. Never committed.
#[test]
fn a_real_engine_bin_if_at_hand() {
    let Ok(path) = std::env::var("LPA_UPDATE_ENGINE_BIN") else {
        return;
    };
    let piece = std::fs::read(&path).expect("LPA_UPDATE_ENGINE_BIN names a file");
    let e = pack_piece(PieceKind::Engine, &piece);
    prove_piece(PieceKind::Engine, &piece, &e).unwrap();
    println!(
        "{path}: {} bytes raw, {} in the stream, {} raw chunks; {:.1}% of raw",
        piece.len(),
        e.stream.len(),
        e.chunks.iter().filter(|&&c| c == 0).count(),
        e.ratio(piece.len()) * 100.0
    );
}
