//! Ratings from game results: a Bradley–Terry fit on the Elo scale with
//! one fixed anchor (design §9.5: Magic has no external calibration, so
//! the built-in greedy agent is the fixed point).

use std::collections::BTreeMap;

/// One rated game: `a` scored `score` (1, 0.5, 0) against `b`.
pub struct Game<'a> {
    pub a: &'a str,
    pub b: &'a str,
    pub score: f64,
}

/// Rating and a 95% interval half-width per player.
/// Every player also gets one virtual draw against the anchor, a weak
/// prior that keeps a rating finite after a few decisive games.
pub fn fit(games: &[Game<'_>], anchor: &str, anchor_rating: f64) -> BTreeMap<String, (f64, f64)> {
    let names: std::collections::BTreeSet<&str> = games.iter().flat_map(|g| [g.a, g.b]).collect();
    let mut all: Vec<Game<'_>> = games
        .iter()
        .map(|g| Game {
            a: g.a,
            b: g.b,
            score: g.score,
        })
        .collect();
    for n in &names {
        if *n != anchor {
            all.push(Game {
                a: n,
                b: anchor,
                score: 0.5,
            });
        }
    }
    let games = &all[..];
    let mut r: BTreeMap<String, f64> = BTreeMap::new();
    for g in games {
        r.entry(g.a.to_string()).or_insert(anchor_rating);
        r.entry(g.b.to_string()).or_insert(anchor_rating);
    }
    let expect = |ra: f64, rb: f64| 1.0 / (1.0 + 10f64.powf((rb - ra) / 400.0));
    // Gradient ascent on the log-likelihood, one player at a time; the
    // anchor stays put.
    for _ in 0..4000 {
        let mut grad: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        for g in games {
            let e = expect(r[g.a], r[g.b]);
            let ga = grad.entry(g.a).or_insert((0.0, 0.0));
            ga.0 += g.score - e;
            ga.1 += 1.0;
            let gb = grad.entry(g.b).or_insert((0.0, 0.0));
            gb.0 += e - g.score;
            gb.1 += 1.0;
        }
        let mut moved = 0.0f64;
        for (name, (d, n)) in grad {
            if name == anchor {
                continue;
            }
            let step = 32.0 * d / n.max(1.0);
            *r.get_mut(name).expect("rated") += step;
            moved = moved.max(step.abs());
        }
        if moved < 1e-4 {
            break;
        }
    }
    // A player no game links to the anchor floats; report it as rated.
    let mut out = BTreeMap::new();
    for (name, rating) in &r {
        let info: f64 = games
            .iter()
            .filter(|g| g.a == name || g.b == name)
            .map(|g| {
                let p = expect(r[g.a], r[g.b]);
                p * (1.0 - p)
            })
            .sum();
        let k = std::f64::consts::LN_10 / 400.0;
        let half = if name == anchor || info <= 0.0 {
            0.0
        } else {
            1.96 / (k * info.sqrt())
        };
        out.insert(name.clone(), (*rating, half));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_who_wins_three_in_four_is_about_190_above() {
        let mut games = Vec::new();
        for i in 0..400 {
            games.push(Game {
                a: "x",
                b: "greedy",
                score: if i % 4 == 0 { 0.0 } else { 1.0 },
            });
        }
        let r = fit(&games, "greedy", 1500.0);
        let (x, half) = r["x"];
        assert!((x - 1690.0).abs() < 6.0, "{x}");
        assert!(half > 20.0 && half < 60.0, "{half}");
        assert!((r["greedy"].0 - 1500.0).abs() < 1e-9);
    }
}
