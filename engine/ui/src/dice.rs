//! The dice table as the hero sees it (TQ's CDiceManager): the table it sits at, what lies on
//! each bet (everyone's and its own), the chip it picked, who sits there with what each won,
//! and the lines the table's news says.

use c3_game::DiceNews;

/// Bets a table may have.
const BETS: usize = 32;

/// The table the hero sits at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiceView {
    pub npc: u32,
    pub totals: [u64; BETS],
    pub mine: [u64; BETS],
    /// The chip picked (0: none).
    pub chip: u64,
    /// Who sits there and what each last won.
    pub players: Vec<(String, u64)>,
}

impl DiceView {
    fn new(npc: u32) -> Self {
        Self {
            npc,
            totals: [0; BETS],
            mine: [0; BETS],
            chip: 0,
            players: Vec::new(),
        }
    }

    fn clear_bets(&mut self) {
        self.totals = [0; BETS];
        self.mine = [0; BETS];
    }
}

/// What a table's news does to the hero's view of it, and the line it says (if any).
pub(crate) fn news(
    view: &mut Option<DiceView>,
    me: &str,
    npc: u32,
    news: &DiceNews,
) -> Option<String> {
    match news {
        DiceNews::Joined { name } if name == me => *view = Some(DiceView::new(npc)),
        DiceNews::Left { name } if name == me => *view = None,
        _ => {}
    }
    let v = view.as_mut().filter(|v| v.npc == npc)?;
    match news {
        DiceNews::Joined { name } if name != me => {
            if !v.players.iter().any(|(n, _)| n == name) {
                v.players.push((name.clone(), 0));
            }
        }
        DiceNews::Left { name } => v.players.retain(|(n, _)| n != name),
        DiceNews::Open { .. } => {
            v.clear_bets();
            v.players.iter_mut().for_each(|p| p.1 = 0);
            return Some("Please place your bet!".into());
        }
        DiceNews::Closed => return Some("Ready? Go!".into()),
        DiceNews::Chips { bets, mine, cancel } => {
            for &(bet, money) in bets {
                let Some(at) = Some(usize::from(bet)).filter(|b| *b < BETS) else {
                    continue;
                };
                let change = |n: &mut u64| {
                    *n = if *cancel {
                        n.saturating_sub(money)
                    } else {
                        *n + money
                    }
                };
                change(&mut v.totals[at]);
                if *mine {
                    change(&mut v.mine[at]);
                }
            }
        }
        DiceNews::Rolled { dice } => {
            let [a, b, c] = dice.map(u32::from);
            let sum = a + b + c;
            let side = if sum > 10 { "Big" } else { "Small" };
            v.clear_bets();
            return Some(format!("Look! {a} {b} {c}, {sum}! {side}!"));
        }
        DiceNews::Won { name, amount } => match v.players.iter_mut().find(|(n, _)| n == name) {
            Some(p) => p.1 = *amount,
            None => v.players.push((name.clone(), *amount)),
        },
        DiceNews::Missed => return Some("You miss the chance! Keep alert!".into()),
        DiceNews::Offered | DiceNews::Joined { .. } => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_follows_the_table() {
        let mut view = None;
        news(&mut view, "Me", 7, &DiceNews::Joined { name: "Me".into() });
        news(&mut view, "Me", 7, &DiceNews::Joined { name: "Ann".into() });
        let chips = |mine, cancel| DiceNews::Chips {
            bets: vec![(1, 1000)],
            mine,
            cancel,
        };
        news(&mut view, "Me", 7, &chips(true, false));
        news(&mut view, "Me", 7, &chips(false, false));
        let v = view.as_ref().unwrap();
        assert_eq!((v.totals[1], v.mine[1]), (2000, 1000));
        news(
            &mut view,
            "Me",
            7,
            &DiceNews::Won {
                name: "Ann".into(),
                amount: 2000,
            },
        );
        assert_eq!(view.as_ref().unwrap().players, [("Ann".to_owned(), 2000)]);
        let said = news(&mut view, "Me", 7, &DiceNews::Rolled { dice: [6, 5, 1] });
        assert_eq!(said.as_deref(), Some("Look! 6 5 1, 12! Big!"));
        news(&mut view, "Me", 7, &DiceNews::Left { name: "Me".into() });
        assert!(view.is_none());
    }
}
