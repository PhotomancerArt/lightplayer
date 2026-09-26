//! The eight data masks and the penalty that picks one.
//!
//! A mask flips the data modules where its condition holds, to break up
//! patterns a scanner could confuse with the symbol's own structure. The
//! encoder tries all eight and keeps the lowest penalty, scored by the
//! standard's four rules:
//!
//! 1. a run of five or more same-coloured modules in a row or column:
//!    3, plus 1 per module past five;
//! 2. each 2×2 block of one colour: 3 (blocks may overlap);
//! 3. each 1:1:3:1:1 finder-like run (dark, light, dark×3, light, dark)
//!    with four light modules on either side: 40 — the area outside the
//!    symbol is the light quiet zone;
//! 4. 10 for every whole 5 % the dark share strays from 50 %.

/// Whether mask `mask` (0–7) flips the module at `row`, `col`.
pub fn flips(mask: u8, row: usize, col: usize) -> bool {
    let (i, j) = (row, col);
    match mask {
        0 => (i + j) % 2 == 0,
        1 => i % 2 == 0,
        2 => j % 3 == 0,
        3 => (i + j) % 3 == 0,
        4 => (i / 2 + j / 3) % 2 == 0,
        5 => (i * j) % 2 + (i * j) % 3 == 0,
        6 => ((i * j) % 2 + (i * j) % 3) % 2 == 0,
        7 => ((i + j) % 2 + (i * j) % 3) % 2 == 0,
        _ => panic!("QR mask {mask} is outside 0..=7"),
    }
}

/// The penalty score of a finished symbol (`modules[row][col]`, true =
/// dark).
pub fn penalty(modules: &[Vec<bool>]) -> u32 {
    let side = modules.len();
    let column = |col: usize| -> Vec<bool> { modules.iter().map(|row| row[col]).collect() };
    let mut score = 0;
    for index in 0..side {
        score += line_penalty(&modules[index]);
        score += line_penalty(&column(index));
    }
    score + block_penalty(modules) + balance_penalty(modules)
}

/// Rules 1 and 3 along one row or column.
fn line_penalty(line: &[bool]) -> u32 {
    let mut score = 0;
    // Rule 1: runs.
    let mut run = 1;
    for pair in line.windows(2) {
        if pair[0] == pair[1] {
            run += 1;
        } else {
            score += run_penalty(run);
            run = 1;
        }
    }
    score += run_penalty(run);
    // Rule 3: the finder-like run, with four light modules on one side.
    // Pad with the quiet zone so the pattern is found at the edges too.
    const FINDER: [bool; 7] = [true, false, true, true, true, false, true];
    let mut padded = vec![false; 4];
    padded.extend_from_slice(line);
    padded.extend([false; 4]);
    for start in 4..=padded.len() - 11 {
        if padded[start..start + 7] != FINDER {
            continue;
        }
        let before = padded[start - 4..start].iter().all(|&dark| !dark);
        let after = padded[start + 7..start + 11].iter().all(|&dark| !dark);
        score += 40 * (u32::from(before) + u32::from(after));
    }
    score
}

fn run_penalty(run: usize) -> u32 {
    if run >= 5 { 3 + (run as u32 - 5) } else { 0 }
}

/// Rule 2: every 2×2 block of one colour.
fn block_penalty(modules: &[Vec<bool>]) -> u32 {
    let mut score = 0;
    for rows in modules.windows(2) {
        for col in 0..rows[0].len() - 1 {
            let color = rows[0][col];
            if rows[0][col + 1] == color && rows[1][col] == color && rows[1][col + 1] == color {
                score += 3;
            }
        }
    }
    score
}

/// Rule 4: how far the dark share strays from half, in whole 5 % steps.
fn balance_penalty(modules: &[Vec<bool>]) -> u32 {
    let total = modules.len() * modules.len();
    let dark = modules.iter().flatten().filter(|&&dark| dark).count();
    // |dark/total − 1/2| in whole twentieths.
    let deviation = (dark * 20).abs_diff(total * 10);
    10 * (deviation / total) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_score_from_five() {
        assert_eq!(line_penalty(&[true, true, true, true, false]), 0);
        assert_eq!(line_penalty(&[true; 5]), 3);
        assert_eq!(line_penalty(&[true; 7]), 5);
    }

    #[test]
    fn a_finder_like_run_scores_once_per_light_side() {
        // Light margin on the left inside the line, quiet zone on the
        // right: both sides count.
        let line = [
            false, false, false, false, true, false, true, true, true, false, true,
        ];
        let runs = run_penalty(4) + run_penalty(1) + run_penalty(1) + run_penalty(3);
        assert_eq!(line_penalty(&line), runs + 80);
    }

    #[test]
    fn a_checkerboard_is_perfectly_balanced() {
        let modules: Vec<Vec<bool>> = (0..21)
            .map(|row| (0..21).map(|col| (row + col) % 2 == 0).collect())
            .collect();
        assert_eq!(balance_penalty(&modules), 0);
        assert_eq!(block_penalty(&modules), 0);
    }

    #[test]
    fn an_all_dark_symbol_is_as_unbalanced_as_it_gets() {
        let modules = vec![vec![true; 21]; 21];
        assert_eq!(balance_penalty(&modules), 100);
        assert_eq!(block_penalty(&modules), 3 * 20 * 20);
    }
}
