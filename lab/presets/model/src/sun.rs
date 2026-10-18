//! A stand-in sun for about 47.6° N (Seattle), in local clock time.
//!
//! Not astronomy: a smooth curve through the right extremes (sunset near
//! 21:10 at midsummer, near 16:20 at midwinter), good enough to make
//! "sunset − 15" move through the year the way it does on a house. The
//! product would compute the real thing from the place's latitude and
//! longitude. Integer maths with a small sine table, so the page can mirror
//! it exactly.

use crate::time::Day;

/// Local minute of the day the sun sets on `day`.
pub fn sunset(day: Day) -> i64 {
    // Midpoint 18:45, swing ±2 h 25 min, longest day near day 172.
    1125 + 145 * sine_milli(day.day_of_year() - 81) / 1000
}

/// Local minute of the day the sun rises on `day`.
pub fn sunrise(day: Day) -> i64 {
    // Mirrored about a solar noon of 13:10 (daylight saving's clock).
    2 * 790 - sunset(day) - 20
}

/// sin(2π · d / 365) × 1000, by a 92-entry quarter table.
fn sine_milli(d: i64) -> i64 {
    let d = d.rem_euclid(365);
    // Quarter of the year ≈ 91.25 days; fold into the first quarter.
    let (q, sign) = if d < 92 {
        (d, 1)
    } else if d < 183 {
        (182 - d, 1)
    } else if d < 274 {
        (d - 183, -1)
    } else {
        (364 - d, -1)
    };
    let q = q.clamp(0, 91) as usize;
    sign * QUARTER_SINE[q]
}

/// sin(π/2 · i / 91) × 1000.
const QUARTER_SINE: [i64; 92] = {
    // Generated at compile time from a Taylor series good to < 1/1000.
    let mut t = [0i64; 92];
    let mut i = 0;
    while i < 92 {
        // x in thousandths of a radian.
        let x = (1571 * i as i64) / 91;
        let x2 = x * x / 1000;
        let x3 = x2 * x / 1000;
        let x5 = x3 * x2 / 1000;
        let x7 = x5 * x2 / 1000;
        t[i] = x - x3 / 6 + x5 / 120 - x7 / 5040;
        i += 1;
    }
    t
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Minute;

    #[test]
    fn sunset_is_late_in_june_and_early_in_december() {
        let june = sunset(Minute::at(2026, 6, 21, 0, 0).day());
        let december = sunset(Minute::at(2026, 12, 21, 0, 0).day());
        assert!((1260..1280).contains(&june), "june {june}");
        assert!((975..995).contains(&december), "december {december}");
    }

    #[test]
    fn halloween_sunset_is_late_afternoon() {
        let s = sunset(Minute::at(2026, 10, 31, 0, 0).day());
        assert!((1020..1095).contains(&s), "halloween {s}");
    }
}
