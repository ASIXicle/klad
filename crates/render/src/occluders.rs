use std::collections::BTreeMap;

pub const TOLERANCE: f32 = 1.5;
const LEVEL: f32 = 0.5;

pub type Polygon = Vec<[f32; 2]>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Edge {
    Across(i64, i64),
    Down(i64, i64),
}

fn crossing(alpha: &dyn Fn(i64, i64) -> f32, edge: Edge) -> [f32; 2] {
    let (a, b, from, step) = match edge {
        Edge::Across(x, y) => (alpha(x, y), alpha(x + 1, y), [x as f32, y as f32], [1.0, 0.0]),
        Edge::Down(x, y) => (alpha(x, y), alpha(x, y + 1), [x as f32, y as f32], [0.0, 1.0]),
    };
    let t = match (b - a).abs() > 1e-6 {
        true => ((LEVEL - a) / (b - a)).clamp(0.0, 1.0),
        false => 0.5,
    };
    [from[0] + 0.5 + step[0] * t, from[1] + 0.5 + step[1] * t]
}

fn segments(alpha: &dyn Fn(i64, i64) -> f32, w: i64, h: i64) -> Vec<(Edge, Edge)> {
    (-1..h)
        .flat_map(|y| (-1..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let inside = |x: i64, y: i64| alpha(x, y) >= LEVEL;
            let case = (inside(x, y) as u8) << 3
                | (inside(x + 1, y) as u8) << 2
                | (inside(x + 1, y + 1) as u8) << 1
                | inside(x, y + 1) as u8;
            let (top, right, bottom, left) = (
                Edge::Across(x, y),
                Edge::Down(x + 1, y),
                Edge::Across(x, y + 1),
                Edge::Down(x, y),
            );
            let middle = (alpha(x, y) + alpha(x + 1, y) + alpha(x + 1, y + 1) + alpha(x, y + 1)) * 0.25 >= LEVEL;
            match case {
                0 | 15 => vec![],
                1 | 14 => vec![(left, bottom)],
                2 | 13 => vec![(bottom, right)],
                3 | 12 => vec![(left, right)],
                4 | 11 => vec![(top, right)],
                6 | 9 => vec![(top, bottom)],
                7 | 8 => vec![(left, top)],
                5 if middle => vec![(left, top), (bottom, right)],
                5 => vec![(left, bottom), (top, right)],
                10 if middle => vec![(left, bottom), (top, right)],
                _ => vec![(left, top), (bottom, right)],
            }
        })
        .collect()
}

pub fn trace(alpha: &[f32], w: usize, h: usize) -> Vec<Polygon> {
    let at = |x: i64, y: i64| -> f32 {
        match x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            true => alpha[y as usize * w + x as usize],
            false => 0.0,
        }
    };
    let pieces = segments(&at, w as i64, h as i64);
    let mut by_edge: BTreeMap<Edge, Vec<usize>> = BTreeMap::new();
    pieces.iter().enumerate().for_each(|(i, (a, b))| {
        by_edge.entry(*a).or_default().push(i);
        by_edge.entry(*b).or_default().push(i);
    });
    let mut used = vec![false; pieces.len()];
    let rings: Vec<Polygon> = (0..pieces.len())
        .filter_map(|start| {
            if used[start] {
                return None;
            }
            used[start] = true;
            let (first, mut here) = pieces[start];
            let mut ring = vec![crossing(&at, first)];
            while here != first {
                ring.push(crossing(&at, here));
                let next = by_edge
                    .get(&here)
                    .and_then(|list| list.iter().copied().find(|i| !used[*i]))?;
                used[next] = true;
                let (a, b) = pieces[next];
                here = if a == here { b } else { a };
            }
            Some(ring)
        })
        .collect();
    rings
        .iter()
        .enumerate()
        .map(|(i, ring)| {
            let depth = rings
                .iter()
                .enumerate()
                .filter(|(j, other)| *j != i && contains(other, ring[0]))
                .count();
            let outward = area(ring) > 0.0;
            match outward == (depth % 2 == 0) {
                true => ring.clone(),
                false => ring.iter().rev().copied().collect(),
            }
        })
        .collect()
}

fn contains(ring: &[[f32; 2]], p: [f32; 2]) -> bool {
    let n = ring.len();
    (0..n).fold(false, |inside, i| {
        let (a, b) = (ring[i], ring[(i + n - 1) % n]);
        match (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            true => !inside,
            false => inside,
        }
    })
}

pub fn area(ring: &[[f32; 2]]) -> f32 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f32>()
        * 0.5
}

fn away(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = libm::hypotf(dx, dy);
    match len > 1e-9 {
        true => ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len,
        false => libm::hypotf(p[0] - a[0], p[1] - a[1]),
    }
}

fn simplify_open(line: &[[f32; 2]], tolerance: f32) -> Vec<[f32; 2]> {
    if line.len() < 3 {
        return line.to_vec();
    }
    let (a, b) = (line[0], line[line.len() - 1]);
    let (far, most) = line[1..line.len() - 1]
        .iter()
        .enumerate()
        .map(|(i, p)| (i + 1, away(*p, a, b)))
        .fold((0, -1.0f32), |best, next| if next.1 > best.1 { next } else { best });
    match most > tolerance {
        true => {
            let mut left = simplify_open(&line[..=far], tolerance);
            left.pop();
            left.extend(simplify_open(&line[far..], tolerance));
            left
        }
        false => vec![a, b],
    }
}

pub fn simplify(ring: &[[f32; 2]], tolerance: f32) -> Polygon {
    if ring.len() < 4 {
        return ring.to_vec();
    }
    let start = ring[0];
    let opposite = (1..ring.len())
        .max_by(|i, j| {
            let d = |k: usize| libm::hypotf(ring[k][0] - start[0], ring[k][1] - start[1]);
            d(*i).total_cmp(&d(*j))
        })
        .unwrap_or(ring.len() / 2);
    let mut there: Vec<[f32; 2]> = ring[..=opposite].to_vec();
    let mut back: Vec<[f32; 2]> = ring[opposite..].to_vec();
    back.push(start);
    there = simplify_open(&there, tolerance);
    back = simplify_open(&back, tolerance);
    there.pop();
    back.pop();
    there.extend(back);
    there
}

pub fn polygons(alpha: &[f32], w: usize, h: usize, tolerance: f32) -> Vec<Polygon> {
    let least = (tolerance * tolerance * 4.0).max(2.0);
    trace(alpha, w, h)
        .into_iter()
        .filter(|ring| area(ring).abs() >= least)
        .map(|ring| simplify(&ring, tolerance.max(0.0)))
        .filter(|ring| ring.len() >= 3)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(w: usize, h: usize, r_out: f32, r_in: f32) -> Vec<f32> {
        (0..w * h)
            .map(|i| {
                let (x, y) = (
                    (i % w) as f32 + 0.5 - w as f32 / 2.0,
                    (i / w) as f32 + 0.5 - h as f32 / 2.0,
                );
                let d = libm::hypotf(x, y);
                ((r_out - d + 0.5).clamp(0.0, 1.0)) * ((d - r_in + 0.5).clamp(0.0, 1.0))
            })
            .collect()
    }

    #[test]
    fn a_ring_keeps_its_hole() {
        let alpha = disc(64, 64, 24.0, 10.0);
        assert_eq!(polygons(&alpha, 64, 64, TOLERANCE).len(), 2, "the outside and the hole");
        let rings = polygons(&alpha, 64, 64, 0.25);
        assert_eq!(rings.len(), 2);
        let areas: Vec<f32> = rings.iter().map(|r| area(r).abs()).collect();
        let (big, small) = (areas[0].max(areas[1]), areas[0].min(areas[1]));
        let pi = std::f32::consts::PI;
        assert!((big - pi * 24.0 * 24.0).abs() < 60.0, "{big}");
        assert!((small - pi * 10.0 * 10.0).abs() < 30.0, "{small}");
        assert!(
            area(&rings[0]).signum() != area(&rings[1]).signum(),
            "the hole winds the other way"
        );
    }

    #[test]
    fn the_polygon_stays_within_the_tolerance_of_the_edge() {
        let alpha = disc(80, 80, 30.0, -1.0);
        let raw = trace(&alpha, 80, 80);
        assert_eq!(raw.len(), 1);
        [0.5, 1.5, 3.0].iter().for_each(|tolerance| {
            let ring = simplify(&raw[0], *tolerance);
            assert!(ring.len() < raw[0].len(), "simplified");
            raw[0].iter().for_each(|p| {
                let nearest = (0..ring.len())
                    .map(|i| away_segment(*p, ring[i], ring[(i + 1) % ring.len()]))
                    .fold(f32::MAX, f32::min);
                assert!(
                    nearest <= tolerance + 1e-3,
                    "{p:?} is {nearest} px off at tolerance {tolerance}"
                );
            });
            ring.iter().for_each(|p| {
                let r = libm::hypotf(p[0] - 40.0, p[1] - 40.0);
                assert!((r - 30.0).abs() < 1.0, "a corner on the edge: {r}");
            });
        });
        assert_eq!(polygons(&vec![0.0; 100], 10, 10, TOLERANCE).len(), 0);
        let full = polygons(&vec![1.0; 100], 10, 10, TOLERANCE);
        assert_eq!(full.len(), 1, "a picture filled to its edges still closes");
        assert!((area(&full[0]).abs() - 100.0).abs() < 10.0, "{}", area(&full[0]));
    }

    fn away_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = match len2 > 1e-9 {
            true => (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0),
            false => 0.0,
        };
        libm::hypotf(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy))
    }
}
