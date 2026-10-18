//! Isometric draw order for objects with cell footprints.
//! Port of the classic C3 insertion sort `CInteractiveLayer::ApplyShow`
//! (3dgamemap/InteractiveLayer.cpp). A footprint `(w, h)` covers cells
//! `x-w+1..=x` × `y-h+1..=y` from the object's cell `(x, y)`.
//!
//! Overlapping footprints are ordered by the axis they share; others fall back to
//! isometric depth `x + y`. Not a total order, so the incremental insertion is kept
//! exactly as the client does it.

use crate::CellPos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    pub cell: CellPos,
    pub size: (u32, u32),
}

/// Returns indices into `items` in back-to-front draw order.
pub fn draw_order(items: &[Footprint]) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::with_capacity(items.len());
    for new in 0..items.len() {
        insert(&mut order, items, new);
    }
    order
}

fn depth(f: &Footprint) -> i32 {
    f.cell.0 + f.cell.1
}

fn insert(order: &mut Vec<usize>, items: &[Footprint], new: usize) {
    let n = &items[new];
    let (nx, ny) = n.cell;
    let (nw, nh) = (n.size.0.max(1) as i32, n.size.1.max(1) as i32);
    let mut overlay_after: Option<usize> = None;

    // Walks back from `i` past objects in the old object's row or nearer than the new one.
    let insert_before = |order: &Vec<usize>, i: usize, old: &Footprint, floor: Option<usize>| {
        let mut at = i;
        for k in (0..i).rev() {
            let o = &items[order[k]];
            if (depth(old) == depth(o) || depth(n) < depth(o)) && floor.is_none_or(|f| k > f) {
                at = k;
            } else {
                break;
            }
        }
        at
    };
    // Walks forward from `i` past objects in the old object's row or farther than the new one.
    let overlay_end = |order: &Vec<usize>, i: usize, old: &Footprint| {
        let mut last = i;
        for (k, &idx) in order.iter().enumerate().skip(i + 1) {
            let o = &items[idx];
            if depth(old) == depth(o) || depth(n) > depth(o) {
                last = k;
            } else {
                break;
            }
        }
        last
    };

    let mut i = 0;
    while i < order.len() {
        let old = &items[order[i]];
        let (ox, oy) = old.cell;
        let (ow, oh) = (old.size.0.max(1) as i32, old.size.1.max(1) as i32);
        let x_separate = (nx - nw + 1) - ox > 0 || (ox - ow + 1) - nx > 0;
        let y_separate = (ny - nh + 1) - oy > 0 || (oy - oh + 1) - ny > 0;
        // Overlap on one axis decides by the other coordinate.
        let decide = if x_separate {
            (!y_separate).then_some(nx < ox)
        } else {
            Some(ny < oy)
        };
        match decide {
            Some(true) => {
                let at = insert_before(order, i, old, overlay_after);
                order.insert(at, new);
                return;
            }
            Some(false) => overlay_after = Some(overlay_end(order, i, old)),
            None => {}
        }
        i += 1;
    }
    match overlay_after {
        Some(after) => order.insert(after + 1, new),
        None => {
            // No overlap with anything: first object that is deeper, else last.
            let at = order
                .iter()
                .position(|&o| depth(n) < depth(&items[o]))
                .unwrap_or(order.len());
            order.insert(at, new);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: i32, y: i32, w: u32, h: u32) -> Footprint {
        Footprint {
            cell: (x, y),
            size: (w, h),
        }
    }

    #[test]
    fn separate_objects_sort_by_depth() {
        let items = [f(10, 10, 1, 1), f(2, 3, 1, 1), f(5, 5, 1, 1)];
        assert_eq!(draw_order(&items), vec![1, 2, 0]);
    }

    #[test]
    fn overlapping_footprints_use_shared_axis() {
        // Same x column, overlapping: smaller y draws first.
        let items = [f(5, 9, 1, 3), f(5, 7, 1, 1)];
        assert_eq!(draw_order(&items), vec![1, 0]);
    }

    #[test]
    fn every_item_appears_once() {
        let items: Vec<_> = (0..50)
            .map(|i| {
                f(
                    (i * 7) % 13,
                    (i * 5) % 11,
                    (i % 3) as u32 + 1,
                    (i % 4) as u32 + 1,
                )
            })
            .collect();
        let mut order = draw_order(&items);
        order.sort_unstable();
        assert_eq!(order, (0..50).collect::<Vec<_>>());
    }
}
