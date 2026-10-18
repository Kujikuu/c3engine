//! Grid movement: the eight step directions and A* path finding.
//!
//! Directions follow the classic C3 numbering (`_DELTA_X/_DELTA_Y`): 0 = +y, then clockwise
//! on screen through −x (2), −y (4) and +x (6). A step may go to any of the eight
//! neighbours that is in the grid, not blocked, and within `max_climb` height of the
//! current cell, the rule the classic client's path finder (`CGameMap::CheckNode`) uses.

use crate::{CellPos, Grid};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Cell offset of each direction.
pub const DIRECTIONS: [CellPos; 8] = [
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
];

/// Direction of a one-cell step from `from` to `to`, if they are neighbours.
pub fn direction(from: CellPos, to: CellPos) -> Option<u8> {
    let delta = (to.0 - from.0, to.1 - from.1);
    DIRECTIONS.iter().position(|&d| d == delta).map(|i| i as u8)
}

/// Direction that best faces from `from` towards `to` (any distance); `None` if equal.
pub fn facing(from: CellPos, to: CellPos) -> Option<u8> {
    let delta = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    direction((0, 0), delta)
}

/// Path finding rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathRules {
    /// Largest height difference one step may cross.
    pub max_climb: i32,
    /// Cells expanded before giving up on reaching the goal.
    pub max_nodes: usize,
}

impl Default for PathRules {
    fn default() -> Self {
        Self {
            max_climb: 25,
            max_nodes: 20_000,
        }
    }
}

/// Whether one step from `from` to its neighbour `to` is allowed.
pub fn can_step(grid: &Grid, from: CellPos, to: CellPos, rules: &PathRules) -> bool {
    match (grid.get(from), grid.get(to)) {
        (Some(a), Some(b)) => {
            !b.is_blocked() && (i32::from(a.height) - i32::from(b.height)).abs() <= rules.max_climb
        }
        _ => false,
    }
}

/// The walkable cell nearest `cell` (itself if walkable), searching outwards in square
/// rings up to `radius` cells away.
pub fn nearest_walkable(grid: &Grid, cell: CellPos, radius: i32) -> Option<CellPos> {
    (0..=radius).find_map(|r| {
        (-r..=r)
            .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| dx.abs() == r || dy.abs() == r)
            .map(|(dx, dy)| (cell.0 + dx, cell.1 + dy))
            .find(|&c| grid.get(c).is_some_and(|c| !c.is_blocked()))
    })
}

/// Octile distance (straight steps cost 10, diagonal 14).
fn distance(a: CellPos, b: CellPos) -> u32 {
    let (dx, dy) = ((a.0 - b.0).unsigned_abs(), (a.1 - b.1).unsigned_abs());
    10 * dx.max(dy) + 4 * dx.min(dy)
}

/// Shortest path from `start` to `goal` as the cells stepped onto (excluding `start`).
/// When `goal` can't be reached, leads to the reachable cell nearest it, as the classic
/// client walks towards unreachable targets. Empty when no step brings it closer.
pub fn find_path(grid: &Grid, start: CellPos, goal: CellPos, rules: &PathRules) -> Vec<CellPos> {
    if grid.get(start).is_none() {
        return Vec::new();
    }
    let mut came_from: HashMap<CellPos, CellPos> = HashMap::new();
    let mut cost: HashMap<CellPos, u32> = HashMap::from([(start, 0)]);
    let mut open = BinaryHeap::from([Reverse((distance(start, goal), start))]);
    let mut nearest = (distance(start, goal), start);
    let mut expanded = 0;
    while let Some(Reverse((_, cell))) = open.pop() {
        if cell == goal {
            nearest = (0, goal);
            break;
        }
        expanded += 1;
        if expanded > rules.max_nodes {
            break;
        }
        let here = cost[&cell];
        for (i, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
            let next = (cell.0 + dx, cell.1 + dy);
            if !can_step(grid, cell, next, rules) {
                continue;
            }
            let step = if i % 2 == 0 { 10 } else { 14 };
            let new_cost = here + step;
            if cost.get(&next).is_some_and(|&c| c <= new_cost) {
                continue;
            }
            cost.insert(next, new_cost);
            came_from.insert(next, cell);
            let h = distance(next, goal);
            nearest = nearest.min((h, next));
            open.push(Reverse((new_cost + h, next)));
        }
    }
    let mut path = Vec::new();
    let mut cell = nearest.1;
    while cell != start {
        path.push(cell);
        cell = came_from[&cell];
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str]) -> Grid {
        let mut g = Grid::new(rows[0].len() as u32, rows.len() as u32);
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                if c == '#' {
                    g.get_mut((x as i32, y as i32)).unwrap().flags = 1;
                }
            }
        }
        g
    }

    #[test]
    fn directions_match_the_classic_table() {
        assert_eq!(direction((5, 5), (5, 6)), Some(0));
        assert_eq!(direction((5, 5), (6, 5)), Some(6));
        assert_eq!(direction((5, 5), (7, 5)), None);
        assert_eq!(facing((0, 0), (10, -3)), Some(5));
    }

    #[test]
    fn walks_around_walls() {
        let g = grid(&[".....", ".###.", "...#.", "....."]);
        let path = find_path(&g, (0, 2), (4, 2), &PathRules::default());
        assert_eq!(path.last(), Some(&(4, 2)));
        let mut at = (0, 2);
        for &next in &path {
            assert!(direction(at, next).is_some() && !g.get(next).unwrap().is_blocked());
            at = next;
        }
        // Around the wall's bottom: (1,3) (2,3) (3,3) then up to (4,2).
        assert_eq!(path.len(), 4, "{path:?}");
    }

    #[test]
    fn unreachable_goal_leads_to_the_nearest_cell() {
        let g = grid(&["..#..", "..#..", "..#.."]);
        let path = find_path(&g, (0, 1), (4, 1), &PathRules::default());
        assert_eq!(path.last(), Some(&(1, 1)));
    }

    #[test]
    fn finds_the_nearest_walkable_cell() {
        let g = grid(&["###", "##.", "###"]);
        assert_eq!(nearest_walkable(&g, (0, 1), 5), Some((2, 1)));
        assert_eq!(nearest_walkable(&g, (2, 1), 5), Some((2, 1)));
        assert_eq!(nearest_walkable(&grid(&["#"]), (0, 0), 3), None);
    }

    #[test]
    fn steep_steps_are_blocked() {
        let mut g = grid(&["..."]);
        g.get_mut((1, 0)).unwrap().height = 40;
        let rules = PathRules::default();
        assert!(!can_step(&g, (0, 0), (1, 0), &rules));
        assert!(find_path(&g, (0, 0), (2, 0), &rules).is_empty());
    }
}
