//! What a data-driven UI's bindings read from the game: the hero's numbers, name and bag.

use crate::GameView;
use crate::gui::GuiData;
use c3_game::Player;

/// What the game UI keeps between frames: the storage NPC whose warehouse is open, and
/// what is typed in its fields.
#[derive(Debug, Default)]
pub(crate) struct GuiState {
    pub storage: Option<u32>,
    pub fields: std::collections::HashMap<String, String>,
    /// The hero's open trade.
    pub trade: Option<TradeView>,
    /// Bag items put on a workbench (compose, gems), in its slots' order.
    pub work: Vec<u32>,
    /// Invites and asks to join a team are taken at once (the team dialog's auto check).
    pub auto_team: bool,
    /// The minimap rolled up.
    pub radar_hidden: bool,
    /// The field whose edit takes the keyboard next frame.
    pub focus: Option<String>,
    /// Each view group's view (`view:<group>:<name>`; a status page's base or details).
    pub views: std::collections::HashMap<String, String>,
    /// The chat bar's lines said, listed names and recent channels.
    pub chat_bar: crate::chat_bar::ChatBar,
    /// The yes/no question shown.
    pub prompt: Option<crate::prompt::Prompt>,
    /// The skill picked for the right button (a type), and a pick waiting for the hero's
    /// skills: the binding naming the skill, and whether a skill aimed at its caster is
    /// cast at once (`use_skill:`) rather than picked (`pick_skill:`).
    pub picked_skill: Option<u32>,
    pub picking: Option<(String, bool)>,
    /// The typed answer's button was pressed.
    pub answering: bool,
    /// The team dialog's Kick was pressed (for the member picked), and the guild's Delete.
    pub kicking: bool,
    pub guild_kicking: bool,
    /// A friend or enemy to let go: which list's picked one.
    pub dropping: Option<String>,
    /// The hotkey page's Change was pressed (for the action picked).
    pub rebinding: bool,
    /// The item picked up from a grid: its list and slot.
    pub held: Option<(String, usize)>,
    /// The other player whose booth the hero was shown.
    pub booth: Option<c3_game::EntityId>,
    /// Words friends left for the hero this session: who and what (TQ's leave-word log).
    pub words: Vec<(String, String)>,
    /// Whom the leave-word dialog writes to.
    pub word_to: String,
    /// A contact list's row double-clicked: whisper an online friend, leave words for an
    /// offline one (in that dialog).
    pub contact_word: Option<(String, u32)>,
    /// A message board button needing the post picked: taking it down, whispering its author.
    pub board_pending: Option<String>,
    /// A whisper waiting to be checked: sent, or (to an offline friend) asked to be left as
    /// words.
    pub whisper: Option<(String, String)>,
    /// A broadcast line typed, waiting for the hero's level to be checked; the lines heard
    /// this session (the board's published pages), the latest last; a waiting line's
    /// button pressed (`first` or `bump`, the row).
    pub broadcast: Option<String>,
    pub broadcasts_heard: Vec<(String, String)>,
    pub pigeon_move: Option<(String, usize)>,
    /// The nobility page's Contribute was pressed, and the face window's OK.
    pub donating: bool,
    pub face_changing: bool,
    /// The shop's repair is on: the next bag item clicked is repaired (TQ's fix cursor).
    pub repairing: bool,
    /// What the player chose to see of the chat (kept with the log's), and each channel's
    /// first colour (the game's look).
    pub chat_filter: crate::ChatFilter,
    pub chat_colors: Vec<(String, usize)>,
    /// The emotions the chat look has (the picker's).
    pub emotes: usize,
    /// An item being bought several at once, and the asking's button pressed (worked out
    /// with the game's data).
    pub purchase: Option<crate::purchase::Purchase>,
    pub purchase_asked: Option<String>,
    /// The lock window's bind pressed.
    pub lock_binding: bool,
    /// The bag chest shown (its item id), and an item kind asked to be used.
    pub chest: Option<u32>,
    pub use_kind: Option<u32>,
    /// A buy asked by an action (`buy:<npc>:<kind>`): its shop and item kind.
    pub buying: Option<(u32, u32)>,
    /// The lines said this session with their channels' keys (the chat log's).
    pub chat_log: Vec<(String, String)>,
    /// The bag item put on the hero's booth, waiting for its price.
    pub booth_item: Option<u32>,
    /// A booth slot's button pressed: taking the ware back (`mine`) or buying it (`theirs`),
    /// and the slot on the page.
    pub booth_slot: Option<(String, usize)>,
    /// View-number steps asked for (`view_step:`): the group, the step and the binding that
    /// bounds it.
    pub steps: Vec<(String, i64, String)>,
}

/// The hero's mentorship, if it has a mentor.
fn world_mentor<'a>(data: &HeroData<'a>) -> Option<&'a c3_game::Mentorship> {
    data.view.sim.world_state().mentors.get(&data.player.name)
}

/// Time at a level as exp balls of 600 (CDlgMentorContribution's MentorContributionExpBalls),
/// two decimals.
fn exp_balls(time: i64) -> String {
    let hundredths = time.max(0) * 1000 / 600 / 10;
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// The lines the chat log keeps.
pub(crate) const CHAT_LOG_KEEP: usize = 1000;

/// Whether the chat log showing `shown` (a channel's key; empty or `all`: every one) shows
/// a line of channel `key` (CChatLog::TestLine).
pub(crate) fn log_shows(shown: &str, key: &str) -> bool {
    matches!(shown, "" | "all") || shown == key
}

/// The dialog showing another player's equipment.
pub(crate) const VIEW_EQUIP_DIALOG: u32 = 258;

/// The emotions a page of the picker shows (CDlgTalkAct: 32).
pub(crate) const EMOTE_PAGE: usize = 32;

/// The faces a page of the face NPC's window shows.
pub(crate) const FACE_PAGE: usize = 6;

/// The nobility board's rows a page shows.
const NOBLE_PAGE: usize = 10;

/// The images of noble rank `rank` (CDlgNobility's RankImages): its icon and title.
fn noble_images(rank: u32) -> (&'static str, &'static str) {
    match rank {
        1 => ("nob_Guard16", "nob_GuardFont32"),
        3 => ("nob_Nobility16", "nob_NobilityFont32"),
        5 => ("nob_Uncle16", "nob_UncleFont32"),
        7 => ("nob_Marquis16", "nob_MarquisFont32"),
        9 => ("nob_Father16", "nob_FatherFont32"),
        12 => ("nob_Infante16", "nob_InfanteFont32"),
        _ => ("nob_White16", "nob_WhiteFont32"),
    }
}

/// The broadcast board's rows a page shows (CDlgBroadcast5065's 10).
const PIGEON_PAGE: usize = 10;

/// The posts a message board's page lists (MsgMessageBoard's lists of 8).
const BOARD_PAGE: usize = 8;

/// The guild members a page lists (CDlgKnightage's MEMBER_PAGE_SIZE).
const GUILD_PAGE: usize = 10;

/// The most items a workbench holds (TQ's oven: 20).
pub(crate) const WORK_SLOTS: usize = 20;

/// A trade as the hero sees it: the partner's goods, its own, the partner's word.
#[derive(Debug)]
pub(crate) struct TradeView {
    pub partner: c3_game::EntityId,
    pub theirs: Vec<u32>,
    pub their_money: u64,
    /// The bag items the hero offers (their ids).
    pub mine: Vec<u32>,
    pub agreed: bool,
    /// The money the hero offers, as the game took it, and whether it agreed.
    pub own_money: u64,
    pub i_agreed: bool,
}

impl GuiState {
    /// What the hero is told about its trade.
    pub fn trade_news(&mut self, partner: c3_game::EntityId, news: &c3_game::TradeNews) {
        use c3_game::TradeNews as N;
        match news {
            N::Opened => {
                self.trade = Some(TradeView {
                    partner,
                    theirs: Vec::new(),
                    their_money: 0,
                    mine: Vec::new(),
                    agreed: false,
                    own_money: 0,
                    i_agreed: false,
                })
            }
            N::Done | N::Closed => self.trade = None,
            _ => {}
        }
        let Some(t) = &mut self.trade else {
            return;
        };
        match news {
            N::PartnerItem(item) => t.theirs.push(item.kind),
            N::PartnerMoney(n) => t.their_money = *n,
            N::PartnerAccepted => t.agreed = true,
            N::OwnMoney(n) => t.own_money = *n,
            N::Refused(item) => t.mine.retain(|i| i != item),
            _ => {}
        }
    }
}

/// The hero, as bindings see it (`hero.life`, `hero.bag`, ...).
pub(crate) struct HeroData<'a> {
    pub view: &'a GameView<'a>,
    pub player: &'a Player,
    pub state: &'a GuiState,
}

impl HeroData<'_> {
    /// The hero's own items a list shows, in slot order: its bag, what it wears (all, or in
    /// slot `n`: `hero.worn.<n>`), what it offers in a trade, a workbench slot, the open
    /// warehouse; none for a list of item types (a shop's).
    /// The hero's skills in group `group`, in the order it learned them.
    fn skills_in<'b>(&'b self, group: &'b str) -> impl Iterator<Item = u32> + 'b {
        let rules = self.view.sim.rules();
        (self.player.skills.iter().copied())
            .filter(move |&s| rules.skill(s).is_some_and(|t| t.group == group))
    }

    /// The hero's guild's members by name (its leader first), none outside a guild.
    pub(crate) fn guild_members(&self) -> Vec<String> {
        let world = self.view.sim.world_state();
        let Some(g) = (world.guild_of(&self.player.name)).and_then(|(id, _)| world.guilds.get(&id))
        else {
            return Vec::new();
        };
        let others = g.members.keys().filter(|m| **m != g.leader).cloned();
        std::iter::once(g.leader.clone()).chain(others).collect()
    }

    /// The groups of the actions players may rebind, in their order.
    fn hotkey_groups(&self) -> Vec<c3_core::input::ActionGroup> {
        let mut groups = Vec::new();
        for row in self.view.controls.rows.iter().filter(|r| r.rebindable) {
            let group = row.action.group();
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        groups
    }

    /// The rebindable actions of the group picked on the hotkey page (`view:hotkey_group`).
    pub(crate) fn hotkey_rows(&self) -> Vec<&crate::ControlRow> {
        let Some(group) = self
            .hotkey_groups()
            .get(self.view_number("hotkey_group"))
            .copied()
        else {
            return Vec::new();
        };
        (self.view.controls.rows.iter())
            .filter(|r| r.rebindable && r.action.group() == group)
            .collect()
    }

    /// The action picked on the hotkey page (`view:hotkey_pick`).
    pub(crate) fn hotkey_picked(&self) -> Option<&crate::ControlRow> {
        let row: usize = self.state.views.get("hotkey_pick")?.parse().ok()?;
        self.hotkey_rows().get(row).copied()
    }

    /// The hero's friends (online first) or enemies (`list`: `friends`, `enemies`), by name.
    pub(crate) fn contacts(&self, list: &str) -> Vec<String> {
        let world = self.view.sim.world_state();
        let names = match list {
            "friends" => world.friends_of(&self.player.name),
            "enemies" => world.enemies_of(&self.player.name).to_vec(),
            "partners" => (world.partners_of(&self.player.name).into_iter())
                .map(|(name, _)| name)
                .collect(),
            // The hero's students, and its mentor (one name).
            "students" => world.students_of(&self.player.name),
            "mentor" => (world.mentors.get(&self.player.name))
                .map(|m| vec![m.mentor.clone()])
                .unwrap_or_default(),
            _ => return Vec::new(),
        };
        let online = |n: &String| {
            world.online.contains_key(n) || self.view.sim.players().any(|p| &p.name == n)
        };
        let (on, off): (Vec<String>, Vec<String>) = names.into_iter().partition(online);
        on.into_iter().chain(off).collect()
    }

    /// The friend or enemy picked in its list (`view:<list>_pick:<row>`), by name.
    pub(crate) fn picked_contact(&self, list: &str) -> Option<String> {
        let row: usize = self
            .state
            .views
            .get(&format!("{list}_pick"))?
            .parse()
            .ok()?;
        self.contacts(list).into_iter().nth(row)
    }

    /// The guild member picked on the member page (`view:guild_pick:<row>`), by name.
    pub(crate) fn picked_guild_member(&self) -> Option<String> {
        let row: usize = self.state.views.get("guild_pick")?.parse().ok()?;
        let first = self.view_number("guild_member_page");
        self.guild_members().into_iter().nth(first + row)
    }

    /// The hero's team, the leader first: as a server sends it, else as the local game has it.
    pub(crate) fn team(&self) -> Vec<c3_game::Mate> {
        match self.player.mates.is_empty() {
            true => self.view.sim.team_mates(self.player.id),
            false => self.player.mates.clone(),
        }
    }

    /// The team member picked in the team dialog (`view:team_pick:<n>`), unless it leads.
    pub(crate) fn picked_member(&self) -> Option<c3_game::Mate> {
        let n = self.view_number("team_pick");
        let picked = self
            .state
            .views
            .get("team_pick")
            .and(self.team().get(n).cloned())?;
        (picked.team != Some(picked.id)).then_some(picked)
    }

    /// The wares of the hero's own booth (`mine`) or of the one it was shown (`theirs`).
    pub(crate) fn booth_wares(&self, whose: &str) -> &[c3_game::Ware] {
        let owner = match whose {
            "mine" => Some(self.player),
            "theirs" => self.state.booth.and_then(|o| self.view.sim.player(o)),
            _ => None,
        };
        owner
            .and_then(|p| p.booth.as_ref())
            .map_or(&[], |b| &b.wares)
    }

    /// The ware in slot `n` of booth `whose`'s page (`view:booth_<whose>_page`).
    pub(crate) fn booth_ware(&self, whose: &str, n: usize) -> Option<&c3_game::Ware> {
        let first = self.view_number(&format!("booth_{whose}_page"));
        self.booth_wares(whose).get(first + n)
    }

    /// The words picked in the received-words dialog (`view:word`): who left them and what.
    fn word_shown(&self) -> Option<&(String, String)> {
        self.state.words.get(self.view_number("word"))
    }

    /// The item picked in the lock window, while carried.
    fn lock_item(&self) -> Option<&c3_game::Item> {
        let id: u32 = self.state.views.get("lock_item")?.parse().ok()?;
        (self.player.bag.iter().chain(&self.player.equipment)).find(|i| i.id == id)
    }

    /// The auto-path heading whose places are shown.
    fn autorun_open(&self) -> Option<u32> {
        self.state.views.get("autorun_open")?.parse().ok()
    }

    /// How many the open chest holds, while it is in the bag.
    fn chest_size(&self) -> Option<u32> {
        let chest = self.state.chest?;
        let kind = self.player.bag.iter().find(|i| i.id == chest)?.kind;
        self.view.sim.chest_size(kind)
    }

    /// The player whose equipment is shown (`view:viewed`), while in the world.
    pub(crate) fn viewed(&self) -> Option<&c3_game::Player> {
        let id = self.state.views.get("viewed")?.parse().ok()?;
        self.view.sim.player(c3_game::EntityId(id))
    }

    /// The faces the hero's body may take (the settings' `faces`; a body none names, as the
    /// game's usual look, the first set's).
    pub(crate) fn faces(&self) -> Vec<u32> {
        let body = self.player.look.body;
        let sets = &self.view.sim.settings().faces;
        let set = (sets.iter())
            .find(|[b0, b1, _, _]| (*b0..=*b1).contains(&body))
            .or(sets.first());
        set.map_or_else(Vec::new, |[_, _, f0, f1]| (*f0..=*f1).collect())
    }

    /// The broadcast board's rows on the page shown (`view:pigeon`: 0 waiting, 1 the hero's
    /// waiting, 2 heard, 3 the hero's heard; `view:pigeon_page`): place, name, line, and the
    /// seconds left (waiting) and line id.
    pub(crate) fn pigeon_rows(&self) -> Vec<(usize, String, String, i64, u32)> {
        let mode = self.view_number("pigeon");
        let first = self.view_number("pigeon_page") * PIGEON_PAGE;
        let me = &self.player.name;
        let now = chrono::Utc::now().timestamp();
        match mode {
            0 | 1 => (self.view.sim.world_state().broadcasts.iter())
                .filter(|l| l.at > now)
                .enumerate()
                .filter(|(_, l)| mode == 0 || l.author == *me)
                .skip(first)
                .take(PIGEON_PAGE)
                .map(|(i, l)| (i + 1, l.author.clone(), l.text.clone(), l.at - now, l.id))
                .collect(),
            _ => (self.state.broadcasts_heard.iter().rev().enumerate())
                .filter(|(_, (name, _))| mode == 2 || name == me)
                .skip(first)
                .take(PIGEON_PAGE)
                .map(|(i, (name, text))| (i + 1, name.clone(), text.clone(), 0, 0))
                .collect(),
        }
    }

    /// The posts of the message board shown (`view:board`), the newest first.
    fn board_posts(&self) -> &[c3_game::Post] {
        let sim = self.view.sim;
        let board = u8::try_from(self.view_number("board")).unwrap_or(0);
        (sim.board_key(board, &self.player.name))
            .and_then(|key| sim.world_state().boards.get(&key))
            .map_or(&[][..], Vec::as_slice)
    }

    /// The post picked on the board's page (`view:board_pick`).
    pub(crate) fn board_picked(&self) -> Option<&c3_game::Post> {
        let row: usize = self.state.views.get("board_pick")?.parse().ok()?;
        self.board_posts().get(self.view_number("board_page") + row)
    }

    /// The hours of trial left with trade partner `name` (rounded up; 0 once formal), as
    /// CDlgBusiness's assessment icons tell them.
    fn trial_hours(&self, name: &str) -> i64 {
        let world = self.view.sim.world_state();
        let probation = self.view.sim.settings().partner_probation_secs;
        let Some((_, since)) =
            (world.partners_of(&self.player.name).into_iter()).find(|(n, _)| n == name)
        else {
            return 0;
        };
        let left = since + probation - chrono::Utc::now().timestamp();
        (left.max(0) + 3599) / 3600
    }

    /// Whether `name` is the hero's friend or trade partner and offline (words may be left for
    /// it).
    pub(crate) fn offline_friend(&self, name: &str) -> bool {
        let world = self.view.sim.world_state();
        let me = &self.player.name;
        (world.are_friends(me, name) || world.are_partners(me, name))
            && !world.online.contains_key(name)
            && self.view.sim.players().all(|p| p.name != name)
    }

    /// A booth slot binding `booth.<whose>.<field>.<n>`: whose booth and the slot.
    fn booth_slot<'k>(key: &'k str, field: &str) -> Option<(&'k str, usize)> {
        let (whose, rest) = key.strip_prefix("booth.")?.split_once('.')?;
        let n = rest.strip_prefix(field)?.strip_prefix('.')?.parse().ok()?;
        Some((whose, n))
    }

    pub(crate) fn own_items(&self, key: &str) -> Option<Vec<&c3_game::Item>> {
        let p = self.player;
        let sim = self.view.sim;
        let in_bag = |id: &u32| p.bag.iter().find(|i| i.id == *id);
        Some(match key {
            "hero.bag" => p.bag.iter().collect(),
            "hero.equipment" => p.equipment.iter().collect(),
            // The item picked in the lock window.
            "lock.item" => self.lock_item().into_iter().collect(),
            // What the open chest holds.
            "chest.items" => (p.chests.get(&self.state.chest?))
                .map_or_else(Vec::new, |items| items.iter().collect()),
            // The shown player's worn item in slot `n` (CDlgNpcEquip).
            _ if key.starts_with("viewed.worn.") => {
                let slot: u8 = key.strip_prefix("viewed.worn.")?.parse().ok()?;
                (self.viewed()?.equipment.iter())
                    .filter(|i| sim.slot_of(i) == Some(slot))
                    .collect()
            }
            _ if key.starts_with("hero.worn.") => {
                let slot: Option<u8> = key.strip_prefix("hero.worn.").and_then(|s| s.parse().ok());
                (p.equipment.iter())
                    .filter(|i| slot.is_some() && sim.slot_of(i) == slot)
                    .collect()
            }
            "trade.mine" => (self.state.trade.iter())
                .flat_map(|t| &t.mine)
                .filter_map(in_bag)
                .collect(),
            // Every item on the workbench (the oven's grid).
            "work.all" => (self.state.work.iter()).filter_map(in_bag).collect(),
            _ if key.starts_with("work.") => {
                let n: usize = key.strip_prefix("work.")?.parse().ok()?;
                self.state
                    .work
                    .get(n)
                    .and_then(in_bag)
                    .into_iter()
                    .collect()
            }
            "storage.items" => (self.state.storage)
                .and_then(|npc| p.storage.get(&npc))
                .map(|items| items.iter().collect())
                .unwrap_or_default(),
            // An equipment row of the battle power detail (`power.row.0`).
            _ if key.starts_with("power.row.") => {
                let n: usize = key.strip_prefix("power.row.")?.parse().ok()?;
                let power = self.view.power.as_ref()?;
                power.rows.get(n)?.item.iter().collect()
            }
            // A booth's wares, and the one in a slot of its page (`booth.mine.0`).
            "booth.mine" | "booth.theirs" => {
                let whose = key.strip_prefix("booth.")?;
                self.booth_wares(whose).iter().map(|w| &w.item).collect()
            }
            _ if key.starts_with("booth.") => {
                let (whose, n) = key.strip_prefix("booth.")?.split_once('.')?;
                let ware = self.booth_ware(whose, n.parse().ok()?);
                ware.map(|w| &w.item).into_iter().collect()
            }
            _ => return None,
        })
    }

    /// The attribute `hero.attr_at.<prefix>.<low>-<high>.<n>[+<group>]` names: its number and
    /// value.
    fn attr_entry(&self, key: &str) -> Option<(i64, i64)> {
        let spec = key.split_once('.')?.1.split_once('.')?.1;
        let (prefix, rest) = spec.split_once('.')?;
        let (ranges, n) = rest.split_once('.')?;
        // `low-high`, or several between commas (`0-399,600-999999`).
        let ranges: Vec<(i64, i64)> = (ranges.split(','))
            .map(|r| {
                let (low, high) = r.split_once('-')?;
                Some((low.parse().ok()?, high.parse().ok()?))
            })
            .collect::<Option<_>>()?;
        let (n, group) = n.split_once('+').map_or((n, None), |(n, g)| (n, Some(g)));
        let offset = group
            .and_then(|g| self.state.views.get(g))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let n = n.parse::<usize>().ok()? + offset;
        let mut found: Vec<(i64, i64)> = (self.player.attributes.iter())
            .filter_map(|(name, &value)| {
                let number: i64 = name.strip_prefix(prefix)?.parse().ok()?;
                let within = ranges
                    .iter()
                    .any(|&(low, high)| (low..=high).contains(&number));
                (within && value > 0).then_some((number, value))
            })
            .collect();
        found.sort_unstable();
        found.get(n).copied()
    }
}

impl GuiData for HeroData<'_> {
    fn focus(&self) -> Option<&str> {
        self.state.focus.as_deref()
    }

    fn number(&self, key: &str) -> Option<f64> {
        // A binding divided, whole (`team.look.0/10000`: a Conquer face from its look).
        if let Some((binding, by)) = key.rsplit_once('/')
            && let Ok(by) = by.parse::<f64>()
            && by != 0.0
        {
            return Some((self.number(binding)? / by).floor());
        }
        let p = self.player;
        let need = || self.view.sim.rules().level_exp(p.level);
        Some(match key {
            "hero.level" => p.level.into(),
            "hero.life" => p.life.into(),
            "hero.life_max" => p.stats.max_life.into(),
            "hero.mana" => p.mana.into(),
            "hero.mana_max" => p.stats.max_mana.into(),
            "hero.exp" => p.exp as f64,
            "hero.exp_next" => need()? as f64,
            "hero.money" => p.money as f64,
            // The hero's money of currency `<name>` (Conquer's CPs: `hero.wallet.CPs`).
            _ if key.starts_with("hero.wallet.") => {
                let name = key.strip_prefix("hero.wallet.")?;
                p.wallet.get(name).copied().unwrap_or(0) as f64
            }
            "hero.running" => f64::from(u8::from(p.running)),
            "ui.radar" => f64::from(u8::from(!self.state.radar_hidden)),
            "hero.bag_count" => p.bag.len() as f64,
            "hero.bag_size" => self.view.sim.bag_size() as f64,
            "hero.attack_min" => p.stats.attack_min.into(),
            "hero.attack_max" => p.stats.attack_max.into(),
            "hero.defence" => p.stats.defence.into(),
            "hero.dodge" => p.stats.dodge.into(),
            "hero.dexterity" => p.stats.dexterity.into(),
            "hero.pk" => p.pk.into(),
            // The storage NPC whose warehouse is open (0: none), and the money kept.
            "hero.storage" => self.state.storage.unwrap_or(0).into(),
            "storage.money" => p.saved_money as f64,
            // 1 while the hero trades; the partner's money and word (1: agreed).
            "hero.trading" => u8::from(self.state.trade.is_some()).into(),
            "trade.money" => self.state.trade.as_ref().map_or(0, |t| t.their_money) as f64,
            "trade.agreed" => u8::from(self.state.trade.as_ref().is_some_and(|t| t.agreed)).into(),
            // The hero's money offer is made, it agreed; the partner's level.
            "trade.money_set" => {
                u8::from(self.state.trade.as_ref().is_some_and(|t| t.own_money > 0)).into()
            }
            "trade.i_agreed" => {
                u8::from(self.state.trade.as_ref().is_some_and(|t| t.i_agreed)).into()
            }
            "trade.partner_level" => {
                let partner = self.state.trade.as_ref()?.partner;
                self.view.sim.player(partner)?.level.into()
            }
            // 1 while the hero talks with an NPC.
            "hero.talking" => u8::from(p.dialogue.is_some()).into(),
            // The most characters the typed answer takes.
            "dialogue.input_limit" => {
                let page = &p.dialogue.as_ref()?.page;
                page.options.iter().find_map(|o| o.input)?.into()
            }
            // 1 when an answer of the NPC's page is typed (a text box).
            "dialogue.typing" => {
                let page = &p.dialogue.as_ref()?.page;
                u8::from(page.options.iter().any(|o| o.input.is_some())).into()
            }
            // The picture of the NPC's page (its face), when it has one.
            "dialogue.picture" => {
                let page = &p.dialogue.as_ref()?.page;
                page.picture? as f64
            }
            // The NPC whose shop is open (0: none).
            "hero.shop" => p.shop.unwrap_or(0).into(),
            "hero.profession" => p.profession.into(),
            // The hero's team: in one, member `n` there, its options, auto-joining.
            "team.in" => u8::from(p.team.is_some()).into(),
            "team.leads" => u8::from(p.team == Some(p.id)).into(),
            _ if key.starts_with("team.has.") => {
                let n: usize = key.strip_prefix("team.has.")?.parse().ok()?;
                let count = match p.mates.is_empty() {
                    true => self.view.sim.team_mates(p.id).len(),
                    false => p.mates.len(),
                };
                u8::from(n < count).into()
            }
            "team.closed" | "team.shares_money" | "team.shares_items" => {
                let (closed, keeps, shares) = self.view.sim.team_options(p.team?);
                u8::from(match key {
                    "team.closed" => closed,
                    "team.shares_money" => !keeps,
                    _ => shares,
                })
                .into()
            }
            "team.auto" => u8::from(self.state.auto_team).into(),
            // Team member `n`'s look; a member is picked who does not lead.
            _ if key.starts_with("team.look.") => {
                let n: usize = key.strip_prefix("team.look.")?.parse().ok()?;
                self.team().get(n)?.look.into()
            }
            "team.picked_member" => u8::from(self.picked_member().is_some()).into(),
            // The player's music and sound volumes, 0 to 100.
            "prefs.music" => f64::from(self.view.controls.prefs.music * 100.0).round(),
            "prefs.sound" => f64::from(self.view.controls.prefs.sound * 100.0).round(),
            // The hero is in a guild; its rank and what it gave; how many members it has.
            "guild.in" => u8::from(self.view.sim.world_state().guild_of(&p.name).is_some()).into(),
            "guild.rank_value" => self.view.sim.world_state().guild_of(&p.name)?.1 as f64,
            "guild.contribution" => {
                let world = self.view.sim.world_state();
                let (id, _) = world.guild_of(&p.name)?;
                let given = world.guilds.get(&id)?.contributions.get(&p.name).copied();
                given.unwrap_or(0) as f64
            }
            "guild.member_total" => self.guild_members().len() as f64,
            "guild.picked_member" => u8::from(self.picked_guild_member().is_some()).into(),
            // The hero's PK mode is `<mode>` (`hero.pk_mode.free|peace|team|arrest`).
            _ if key.starts_with("hero.pk_mode.") => {
                let mode = key.strip_prefix("hero.pk_mode.")?;
                u8::from(crate::pk_mode_name(p.pk_mode) == mode).into()
            }
            // A question shows, of kind `<kind>` (`prompt.kind.<kind>`).
            "prompt.open" => u8::from(self.state.prompt.is_some()).into(),
            // Words left for the hero: how many, whether there are words before or after
            // those shown, and whether their sender may be answered (an offline friend).
            "words.count" => self.state.words.len() as f64,
            // The broadcast board: a row filled, the hero's own waiting line (its move
            // buttons) or another's (whisper), the mode shown, lines heard.
            _ if key.starts_with("pigeon.") && key.ends_with(".mine") => {
                let n: usize = key
                    .strip_prefix("pigeon.")?
                    .strip_suffix(".mine")?
                    .parse()
                    .ok()?;
                let mode = self.view_number("pigeon");
                let row = self.pigeon_rows().into_iter().nth(n)?;
                u8::from(mode < 2 && row.1 == self.player.name).into()
            }
            _ if key.starts_with("pigeon.") && key.ends_with(".other") => {
                let n: usize = key
                    .strip_prefix("pigeon.")?
                    .strip_suffix(".other")?
                    .parse()
                    .ok()?;
                let row = self.pigeon_rows().into_iter().nth(n)?;
                u8::from(row.1 != self.player.name).into()
            }
            "pigeon.mode" => self.view_number("pigeon") as f64,
            // The chat options: a channel shown (`chat.shown.talk`), whispers from friends only,
            // a channel's colour (`chat.color.team`: an index of the palette), the colour list
            // open.
            _ if key.starts_with("chat.shown.") => {
                let channel = key.strip_prefix("chat.shown.")?;
                u8::from(!self.state.chat_filter.hidden.contains(channel)).into()
            }
            "chat.friends_only" => u8::from(self.state.chat_filter.friends_only).into(),
            _ if key.starts_with("chat.color.") => {
                let channel = key.strip_prefix("chat.color.")?;
                let first = self
                    .state
                    .chat_colors
                    .iter()
                    .find(|(k, _)| k == channel)
                    .map(|(_, i)| *i);
                self.state
                    .chat_filter
                    .colors
                    .get(channel)
                    .copied()
                    .or(first)
                    .unwrap_or(0) as f64
            }
            // The auto-path menu: how many headings, and the row of the one open.
            "autorun.count" => crate::destinations::roots(self.view).len() as f64,
            "autorun.open" => u8::from(self.number("autorun.open_row").is_some()).into(),
            "autorun.open_row" => {
                let open = self.autorun_open()?;
                let roots = crate::destinations::roots(self.view);
                roots.iter().position(|d| d.id == open)? as f64
            }
            // The lock window: an item picked, and its state (0 free, 1 locked, 2 unlocking).
            "lock.picked" => u8::from(self.lock_item().is_some()).into(),
            "lock.state" => match self.lock_item()?.lock {
                0 => 0.0,
                1 => 1.0,
                _ => 2.0,
            },
            // The open chest (while in the bag) and its rows of three.
            "chest.open" => u8::from(self.chest_size().is_some()).into(),
            "chest.rows" => (self.chest_size()? / 3) as f64,
            // The purchase asking: open, its step, the most that may be bought (and whether
            // the funds hold it), how many are to be bought and what they cost.
            "purchase.open" => u8::from(self.state.purchase.is_some()).into(),
            "purchase.step" => self.state.purchase?.step.number().into(),
            "purchase.most" | "purchase.by_funds" | "purchase.cost" | "purchase.count" => {
                let p = self.state.purchase?;
                let l = crate::purchase::limits(self.view, self.player, p.npc, p.kind)?;
                let count = match p.step {
                    crate::purchase::Step::Confirm(n) => n,
                    _ => 0,
                };
                match key {
                    "purchase.most" => l.most.into(),
                    "purchase.by_funds" => u8::from(l.by_funds).into(),
                    "purchase.count" => count.into(),
                    _ => (l.price * u64::from(count)) as f64,
                }
            }
            // The emotion picker's pages.
            "emote.pages" => self.state.emotes.div_ceil(EMOTE_PAGE) as f64,
            "chat.choosing" => u8::from(self.state.views.contains_key("chat_color_for")).into(),
            // The faces the hero may take, and the one picked on the page (`view:face_pick`).
            "faces.count" => self.faces().len() as f64,
            "face.pages" => self.faces().len().div_ceil(FACE_PAGE) as f64,
            _ if key.starts_with("face_picked.") => {
                let n: usize = key.strip_prefix("face_picked.")?.parse().ok()?;
                let picked =
                    (self.state.views.get("face_pick")).and_then(|v| v.parse::<usize>().ok());
                u8::from(picked == Some(n)).into()
            }
            // Battle power: the total, each part (`power.0` the level .. `power.6` other), the
            // detail page's mode (`view:power_mode`: 1 quality .. 5 other) and a row's score,
            // quality step (0 none, 1 Refined .. 4 Super) and whether it counts.
            // The shown player's profession and battle power.
            "viewed.profession" => self.viewed()?.profession as f64,
            "viewed.power" => self.view.viewed_power.as_ref()?.parts.iter().sum::<i64>() as f64,
            "power.total" => self.view.power.as_ref()?.parts.iter().sum::<i64>() as f64,
            "power.mode" => self.view_number("power_mode") as f64,
            // The detail page's total: the part its mode counts.
            "power.mode_total" => {
                let mode = self.view_number("power_mode");
                *self.view.power.as_ref()?.parts.get(mode + 1)? as f64
            }
            _ if key.starts_with("power.row.") => {
                let (n, field) = key.strip_prefix("power.row.")?.split_once('.')?;
                let row = self
                    .view
                    .power
                    .as_ref()?
                    .rows
                    .get(n.parse::<usize>().ok()?)?;
                let mode = self.view_number("power_mode");
                match field {
                    "score" => row.scores.get(mode.checked_sub(1)?).copied().unwrap_or(0) as f64,
                    "q" => {
                        f64::from(row.quality.saturating_sub(5).min(4) * u8::from(row.quality >= 6))
                    }
                    "shadowed" => u8::from(row.shadowed).into(),
                    _ => return None,
                }
            }
            _ if key.starts_with("power.") => {
                let n: usize = key.strip_prefix("power.")?.parse().ok()?;
                *self.view.power.as_ref()?.parts.get(n)? as f64
            }
            // The hero's noble rank and the rank picked to ask about (`view:nobility_query`).
            "nobility.rank" => {
                let sim = self.view.sim;
                f64::from(
                    sim.world_state()
                        .noble_rank(&sim.settings().nobility, &p.name)
                        .0,
                )
            }
            "nobility.query" => self.view_number("nobility_query") as f64,
            "pigeon.rows" => self.pigeon_rows().len() as f64,
            "pigeon.heard" => self.state.broadcasts_heard.len() as f64,
            // The message board: its posts, the mode (0 list, 1 writing, 2 reading), the tab
            // chosen (`board.tab.0`).
            "board.count" => self.board_posts().len() as f64,
            "board.mode" => self.view_number("board_mode") as f64,
            _ if key.starts_with("board.tab.") => {
                let tab: usize = key.strip_prefix("board.tab.")?.parse().ok()?;
                u8::from(self.view_number("board") == tab).into()
            }
            "word.prev" => u8::from(self.view_number("word") > 0).into(),
            // The hero's mentor: whether it has one and is leaving it (the days left), its
            // students and whether it may take in more, a student's row filled, and what its
            // students gave (time, blessed hours, item points).
            "mentor.has" => u8::from(world_mentor(self).is_some()).into(),
            "mentor.leaving" => u8::from(world_mentor(self).is_some_and(|m| m.left != 0)).into(),
            "mentor.leave_days" => {
                let m = world_mentor(self).filter(|m| m.left != 0)?;
                let hours = self.view.sim.settings().mentor.leave_hours;
                let left = m.left + hours - chrono::Utc::now().timestamp() / 3600;
                (left.max(0) / 24 + 1) as f64
            }
            "mentor.students_count" => self.contacts("students").len() as f64,
            "mentor.can_take" => {
                let m = &self.view.sim.settings().mentor;
                let limit = (m.students.iter())
                    .find(|[from, to, _]| (*from..=*to).contains(&p.level))
                    .map_or(0, |r| r[2]) as usize;
                let alive = p.is_alive() && m.min_level > 0;
                u8::from(alive && self.contacts("students").len() < limit).into()
            }
            _ if key.starts_with("mentor.student.") => {
                let n: usize = key.strip_prefix("mentor.student.")?.parse().ok()?;
                u8::from(n < self.contacts("students").len()).into()
            }
            "mentor.pool.time" | "mentor.pool.bless" | "mentor.pool.items" => {
                let pool = (self.view.sim.world_state().mentor_pools.get(&p.name))
                    .copied()
                    .unwrap_or_default();
                match key {
                    "mentor.pool.time" => pool.time as f64,
                    "mentor.pool.bless" => pool.bless as f64,
                    _ => pool.items as f64,
                }
            }
            // The hero's trade partners, and the hours of trial left with the one in row `n`.
            "partners.count" => self.contacts("partners").len() as f64,
            _ if key.starts_with("partner.trial.") => {
                let n: usize = key.strip_prefix("partner.trial.")?.parse().ok()?;
                let name = self.contacts("partners").into_iter().nth(n)?;
                self.trial_hours(&name) as f64
            }
            "word.next" => u8::from(self.view_number("word") + 1 < self.state.words.len()).into(),
            "word.can_reply" => u8::from(
                self.word_shown()
                    .is_some_and(|(from, _)| self.offline_friend(from)),
            )
            .into(),
            // The hero keeps a booth; it was shown another's, still open.
            "booth.mine" | "booth.theirs" => {
                let open = match key {
                    "booth.mine" => p.booth.is_some(),
                    _ => (self.state.booth.and_then(|o| self.view.sim.player(o)))
                        .is_some_and(|o| o.booth.is_some()),
                };
                u8::from(open).into()
            }
            // A booth slot's coin: 0 empty, 1 the money, 2 another currency
            // (`booth.mine.coin.0`).
            _ if Self::booth_slot(key, "coin").is_some() => {
                let (whose, n) = Self::booth_slot(key, "coin")?;
                let coin =
                    (self.booth_ware(whose, n)).map_or(0, |w| 1 + u8::from(!w.currency.is_empty()));
                coin.into()
            }
            // The open shop's wares are priced in currency `<name>` (`shop.priced_in.CPs`; TQ's
            // CP shops).
            _ if key.starts_with("shop.priced_in.") => {
                let currency = key.strip_prefix("shop.priced_in.")?;
                let rules = self.view.sim.rules();
                let Some(npc) = p.shop else {
                    return Some(0.0);
                };
                let first = rules.shop(npc).first().copied();
                let priced = first.and_then(|kind| {
                    let own = rules.item(kind).map(|t| t.currency.as_str());
                    rules.shop_currency(npc, kind).or(own)
                });
                u8::from(priced == Some(currency)).into()
            }
            // The page view group `<group>` shows, `<per>` to a page, from 1
            // (`view_page.20.depot_page`), and how many pages list `<list>` fills, 1 at least
            // (`list_pages.20.storage.items`).
            _ if key.starts_with("view_page.") => {
                let (per, group) = key.strip_prefix("view_page.")?.split_once('.')?;
                let per: usize = per.parse().ok().filter(|&p| p > 0)?;
                (self.view_number(group) / per + 1) as f64
            }
            _ if key.starts_with("list_pages.") => {
                let (per, list) = key.strip_prefix("list_pages.")?.split_once('.')?;
                let per: usize = per.parse().ok().filter(|&p| p > 0)?;
                self.items(list).len().div_ceil(per).max(1) as f64
            }
            // How many items list `<list>` holds (`count.shop.items`).
            _ if key.starts_with("count.") => self.items(key.strip_prefix("count.")?).len() as f64,
            "skill.picked" => u8::from(self.state.picked_skill.is_some()).into(),
            // The skill picked is in group `<group>` (`skill.picked_in.xp`: an XP skill), and how
            // many of its skills the hero has in a group (`skill_count.magic`).
            _ if key.starts_with("skill.picked_in.") => {
                let group = key.strip_prefix("skill.picked_in.")?;
                let skill = self
                    .state
                    .picked_skill
                    .and_then(|s| self.view.sim.rules().skill(s));
                u8::from(skill.is_some_and(|t| t.group == group)).into()
            }
            _ if key.starts_with("skill_count.") => {
                let group = key.strip_prefix("skill_count.")?;
                self.skills_in(group).count() as f64
            }
            // The hero has skills (the strip opens only then).
            "hero.has_skills" => u8::from(!p.skills.is_empty()).into(),
            _ if key.starts_with("prompt.kind.") => {
                let kind = key.strip_prefix("prompt.kind.")?;
                u8::from(self.state.prompt.as_ref().is_some_and(|p| p.kind == kind)).into()
            }
            // The name typed in the chat bar is listed / can be (the Add button's state).
            "chat.listed" | "chat.can_list" => {
                let name = self.state.fields.get(crate::CHAT_NAME);
                let name = name.map_or("", |n| n.trim());
                let bar = &self.state.chat_bar;
                u8::from(match key {
                    "chat.listed" => bar.listed(name),
                    _ => bar.can_list(name),
                })
                .into()
            }
            // 1 while view group `<group>` shows view `<name>` (`view.<group>.<name>`).
            _ if key.starts_with("view.") => {
                let (group, name) = key.strip_prefix("view.")?.split_once('.')?;
                u8::from(self.state.views.get(group).is_some_and(|v| v == name)).into()
            }
            // The `n`th of the hero's attributes named `<prefix><number>` with the number in
            // `low-high` and above 0: its number (`hero.attr_at.<prefix>.<low>-<high>.<n>`) or
            // its value (`hero.attr_val...`); `n` may add a view group's number (`3+page`).
            _ if key.starts_with("hero.attr_at.") || key.starts_with("hero.attr_val.") => {
                let (number, value) = self.attr_entry(key)?;
                (if key.starts_with("hero.attr_at.") {
                    number
                } else {
                    value
                }) as f64
            }
            // A number the game shows of the hero (`hero.shown.<name>`, its rules' `shown`).
            _ if key.starts_with("hero.shown.") => {
                let name = key.strip_prefix("hero.shown.")?;
                let shown = self.view.sim.shown(p.id);
                shown.iter().find(|(n, _)| n == name).map_or(0, |(_, v)| *v) as f64
            }
            // Any of the hero's attributes (`hero.attr.strength`), 0 when it has none.
            _ => {
                // Its own value, else the attribute's default (stamina starts full).
                let name = key.strip_prefix("hero.attr.")?;
                let default = (self.view.sim.settings().attributes.iter())
                    .find(|a| a.name == name)
                    .map_or(0, |a| a.default);
                p.attributes.get(name).copied().unwrap_or(default) as f64
            }
        })
    }

    fn text(&self, key: &str) -> Option<String> {
        match key {
            "hero.name" => Some(self.player.name.clone()),
            // Who asked the question shown, and its kind.
            "prompt.name" => Some(self.state.prompt.as_ref()?.name.clone()),
            "prompt.kind" => Some(self.state.prompt.as_ref()?.kind.clone()),
            // A part of the asker's name holding several values (`a||b`: `prompt.part.1`).
            _ if key.starts_with("prompt.part.") => {
                let n: usize = key.strip_prefix("prompt.part.")?.parse().ok()?;
                let name = &self.state.prompt.as_ref()?.name;
                Some(name.split("||").nth(n).unwrap_or_default().to_owned())
            }
            // The hero's mentor's name; what its students gave, as CDlgMentorContribution
            // words it (time as exp balls of 600, item points as hundredths); the top giver of
            // each kind (`mentor.top.0.name`) and its share.
            "mentor.name" => Some(
                world_mentor(self)
                    .map(|m| m.mentor.clone())
                    .unwrap_or_default(),
            ),
            "mentor.pool.balls" | "mentor.pool.hours" | "mentor.pool.stones" => {
                let pool = (self
                    .view
                    .sim
                    .world_state()
                    .mentor_pools
                    .get(&self.player.name))
                .copied()
                .unwrap_or_default();
                Some(match key {
                    "mentor.pool.balls" => exp_balls(pool.time),
                    "mentor.pool.hours" => pool.bless.to_string(),
                    _ => format!("{}.{:02}", pool.items / 100, pool.items % 100),
                })
            }
            _ if key.starts_with("mentor.top.") => {
                let (kind, field) = key.strip_prefix("mentor.top.")?.split_once('.')?;
                let kind: usize = kind.parse().ok()?;
                let gifts = &self.view.sim.world_state().mentor_gifts;
                let value = |g: &c3_game::Gifts| [g.time, g.bless, g.items][kind.min(2)];
                let top = (gifts.iter())
                    .filter(|((m, _), g)| *m == self.player.name && value(g) > 0)
                    .max_by_key(|(_, g)| value(g));
                Some(match (top, field) {
                    (Some(((_, s), _)), "name") => s.clone(),
                    (Some((_, g)), "value") => match kind {
                        0 => exp_balls(g.time),
                        1 => g.bless.to_string(),
                        _ => format!("{}.{:02}", g.items / 100, g.items % 100),
                    },
                    _ => String::new(),
                })
            }
            // The leave-word dialog's friend and the words shown in the received ones.
            "word.to" => Some(self.state.word_to.clone()),
            "word.from" => Some(self.word_shown().map(|w| w.0.clone()).unwrap_or_default()),
            "word.said" => Some(self.word_shown().map(|w| w.1.clone()).unwrap_or_default()),
            // Nobility: the hero's donation and place, what the rank asked about needs, and
            // the board's rows (`nobility.row.0.name`; on the page `view:nobility_page`).
            "nobility.donation" | "nobility.place" | "nobility.need" => {
                let sim = self.view.sim;
                let (world, n) = (sim.world_state(), &sim.settings().nobility);
                let mine = world
                    .donations
                    .get(&self.player.name)
                    .map_or(0, |d| d.total);
                Some(match key {
                    "nobility.donation" => mine.to_string(),
                    "nobility.place" => match world.noble_rank(n, &self.player.name).1 {
                        Some(place) => (place + 1).to_string(),
                        None => "--".to_owned(),
                    },
                    _ => {
                        let rank = u32::try_from(self.view_number("nobility_query")).unwrap_or(0);
                        world.noble_price(n, rank, mine).to_string()
                    }
                })
            }
            _ if key.starts_with("nobility.row.") => {
                let (n, field) = key.strip_prefix("nobility.row.")?.split_once('.')?;
                let at =
                    self.view_number("nobility_page") * NOBLE_PAGE + n.parse::<usize>().ok()?;
                let sim = self.view.sim;
                let nobles = sim.world_state().nobles(&sim.settings().nobility);
                let Some(noble) = nobles.get(at) else {
                    return Some(String::new());
                };
                let (icon, title) = noble_images(noble.rank);
                Some(match field {
                    "place" => (at + 1).to_string(),
                    "icon" => icon.to_owned(),
                    "title" => title.to_owned(),
                    "name" => noble.name.clone(),
                    "money" => noble.total.to_string(),
                    _ => return None,
                })
            }
            "nobility.pages" => {
                let sim = self.view.sim;
                let count = sim.world_state().nobles(&sim.settings().nobility).len();
                let pages = count.div_ceil(NOBLE_PAGE).max(1);
                Some(format!("{}/{pages}", self.view_number("nobility_page") + 1))
            }
            "nobility.amount" => Some(self.state.fields.get(key).cloned().unwrap_or_default()),
            // The hero's rank title and icon, and the title of the rank asked about.
            // The shown player's name, spouse (none for StrRes 100122, "None") and rank icon.
            "viewed.name" => Some(self.viewed()?.name.clone()),
            "viewed.mate" => {
                let spouse = &self.view.sim.settings().spouse_text;
                Some(
                    self.viewed()?
                        .texts
                        .get(spouse)
                        .cloned()
                        .unwrap_or_default(),
                )
            }
            "viewed.noble" => {
                let sim = self.view.sim;
                let n = &sim.settings().nobility;
                let rank = sim.world_state().noble_rank(n, &self.viewed()?.name).0;
                Some(noble_images(rank).0.replace("16", "64"))
            }
            "nobility.rank_name" | "nobility.query_name" | "nobility.icon" => {
                let sim = self.view.sim;
                let n = &sim.settings().nobility;
                let rank = match key {
                    "nobility.query_name" => {
                        u32::try_from(self.view_number("nobility_query")).unwrap_or(0)
                    }
                    _ => sim.world_state().noble_rank(n, &self.player.name).0,
                };
                Some(match key {
                    "nobility.icon" => noble_images(rank).0.to_owned(),
                    _ => (n.titles.iter())
                        .find(|(r, _)| *r == rank)
                        .map_or_else(String::new, |(_, t)| t.clone()),
                })
            }
            // A battle power detail row's item name (StrRes 10493 when nothing is worn) and
            // what it shows in the page's mode (a StrRes key: quality 10075-10080, sockets
            // 10494/10495, super gems 10496/10497, plus 10498/10499, broken 11055).
            _ if key.starts_with("power.row.") && key.matches('.').count() == 3 => {
                let (n, field) = key.strip_prefix("power.row.")?.split_once('.')?;
                let row = self
                    .view
                    .power
                    .as_ref()?
                    .rows
                    .get(n.parse::<usize>().ok()?)?;
                let mode = self.view_number("power_mode");
                let item = row.item.as_ref();
                Some(match field {
                    "name" => item
                        .and_then(|i| self.view.sim.rules().item(i.kind))
                        .map_or_else(|| "10493".to_owned(), |t| t.name.clone()),
                    "detail" => {
                        let score = row.scores.get(mode.wrapping_sub(1)).copied().unwrap_or(0);
                        let key = match (item, mode) {
                            (None, _) => 10075,
                            (Some(i), _) if i.durability == 0 => 11055,
                            (Some(_), 1) if row.quality >= 6 => 10071 + u32::from(row.quality),
                            (Some(_), 1) => 10076,
                            (Some(_), 2) => 10494 + u32::from(score > 0),
                            (Some(_), 3) => 10496 + u32::from(score > 0),
                            (Some(_), 4) => 10498 + u32::from(score > 0),
                            _ => return Some(String::new()),
                        };
                        key.to_string()
                    }
                    _ => return self.number(key).map(|n| format!("{n}")),
                })
            }
            // The broadcast banner's latest line, and the board's rows (`pigeon.0.name`).
            "pigeon.last.name" | "pigeon.last.text" => {
                let (name, text) = self.state.broadcasts_heard.last()?;
                Some(if key.ends_with("name") {
                    name.clone()
                } else {
                    text.clone()
                })
            }
            _ if key.starts_with("pigeon.") && key.matches('.').count() == 2 => {
                let (n, field) = key.strip_prefix("pigeon.")?.split_once('.')?;
                let row = self.pigeon_rows().into_iter().nth(n.parse().ok()?);
                Some(match (row, field) {
                    (Some(r), "index") => r.0.to_string(),
                    (Some(r), "name") => r.1,
                    (Some(r), "text") => r.2.chars().take(32).collect(),
                    (Some(r), "clock") if r.3 > 0 => format!("{:02}:{:02}", r.3 / 60, r.3 % 60),
                    _ => String::new(),
                })
            }
            "chatlog.date" => Some(chrono::Local::now().format("%Y-%-m-%-d").to_string()),
            // The lock window: the picked item's name, its lock picture (`EquipTie_` and
            // this), and the tip's StrRes key (11194, or 11195 for no equipment).
            "lock.name" => Some(crate::item_name(self.view.sim, self.lock_item()?.kind)),
            "lock.icon" => Some(match self.lock_item()?.lock {
                0 => "UnTieIco".into(),
                _ => "TieIco".into(),
            }),
            "lock.tip" => Some(
                self.state
                    .views
                    .get("lock_tip")
                    .cloned()
                    .unwrap_or_else(|| "11194".into()),
            ),
            // The item being bought: its name, and how many are to be bought.
            "purchase.name" => Some(crate::item_name(self.view.sim, self.state.purchase?.kind)),
            "word.text" | "board.text" | "black.name" | "purchase.amount" => {
                Some(self.state.fields.get(key).cloned().unwrap_or_default())
            }
            // The post read on the message board.
            "board.read" => Some(
                self.board_picked()
                    .map(|p| p.text.clone())
                    .unwrap_or_default(),
            ),
            // A booth slot's price (`booth.theirs.price.0`), empty when the slot is.
            _ if Self::booth_slot(key, "price").is_some() => {
                let (whose, n) = Self::booth_slot(key, "price")?;
                Some(
                    self.booth_ware(whose, n)
                        .map_or_else(String::new, |w| w.price.to_string()),
                )
            }
            // An amount typed in the prompt in a currency (`prompt.amount.CPs`); the cry.
            _ if key.starts_with("prompt.amount.") || key == "booth.cry" => {
                Some(self.state.fields.get(key).cloned().unwrap_or_default())
            }
            // Team member `n`'s face (1, Conquer's heroes'), none past the team's end.
            // The emotion in place `emote.N` of the picker's page, none past the last.
            _ if key.starts_with("emote.") => {
                let n: usize = key.strip_prefix("emote.")?.parse().ok()?;
                let at = self.view_number("emote_page") * EMOTE_PAGE + n;
                (at < self.state.emotes).then(|| at.to_string())
            }
            // A team member's face (its own, else 1), none past the team's end.
            _ if key.starts_with("team.face.") => {
                let n: usize = key.strip_prefix("team.face.")?.parse().ok()?;
                let mate = self.team().into_iter().nth(n)?;
                let face = self.view.sim.player(mate.id).map_or(0, |p| p.look.face);
                Some(if face == 0 { 1 } else { face }.to_string())
            }
            // The faces the hero's body may take, six a page (`view:face_page`): `face.0`.
            _ if key.starts_with("face.") => {
                let n: usize = key.strip_prefix("face.")?.parse().ok()?;
                let at = self.view_number("face_page") * FACE_PAGE + n;
                Some(
                    self.faces()
                        .get(at)
                        .map_or_else(String::new, u32::to_string),
                )
            }
            // The hero's text variable `<name>` (its spouse: `hero.text.mate`).
            _ if key.starts_with("hero.text.") => {
                let name = key.strip_prefix("hero.text.")?;
                Some(self.player.texts.get(name).cloned().unwrap_or_default())
            }
            // The skill picked for the right button (its type).
            "skill.picked" => self.state.picked_skill.map(|s| s.to_string()),
            // The hero's `n`th skill in group `<group>` (`skill_in.xp.0`: its first XP skill).
            _ if key.starts_with("skill_in.") => {
                let (group, n) = key.strip_prefix("skill_in.")?.rsplit_once('.')?;
                let n: usize = n.parse().ok()?;
                Some(self.skills_in(group).nth(n)?.to_string())
            }
            // The hero's skill `n` (its type), none past the last.
            _ if key.starts_with("skill.") => {
                let n: usize = key.strip_prefix("skill.")?.parse().ok()?;
                Some(self.player.skills.get(n)?.to_string())
            }
            "storage.amount" | "trade.amount" | "dialogue.typed" | "prompt.amount"
            | "guild.notice" => Some(self.state.fields.get(key).cloned().unwrap_or_default()),
            // Team member `n`'s name (the leader first), empty past the team's end.
            _ if key.starts_with("team.member.") => {
                let n: usize = key.strip_prefix("team.member.")?.parse().ok()?;
                Some(
                    self.team()
                        .get(n)
                        .map(|m| m.name.clone())
                        .unwrap_or_default(),
                )
            }
            // The keys of the action picked, or that a new key is awaited.
            "hotkey.current" => Some(self.hotkey_picked()?.bindings.clone()),
            "hotkey.listening" => self
                .view
                .controls
                .listening
                .map(|a| format!("{} ...", a.label())),
            // The friend or enemy picked: `contact.<list>.<field>` (name, level, profession,
            // pk, guild, rank, mate); what only a player here shows is empty elsewhere.
            _ if key.starts_with("contact.") => {
                let (list, field) = key.strip_prefix("contact.")?.split_once('.')?;
                let name = self.picked_contact(list)?;
                let world = self.view.sim.world_state();
                let here = self.view.sim.players().find(|p| p.name == name);
                let guild = world.guild_of(&name);
                Some(match field {
                    "name" => name.clone(),
                    "level" => world
                        .levels
                        .get(&name)
                        .map_or_else(String::new, u32::to_string),
                    "profession" => here.map_or_else(String::new, |p| p.profession.to_string()),
                    "pk" => here.map_or_else(String::new, |p| p.pk.to_string()),
                    "guild" => (guild.and_then(|(g, _)| world.guilds.get(&g)))
                        .map_or_else(String::new, |g| g.name.clone()),
                    "rank" => guild.map_or_else(String::new, |(_, r)| r.to_string()),
                    "trial" => self.trial_hours(&name).to_string(),
                    "mate" => {
                        let spouse = &self.view.sim.settings().spouse_text;
                        here.and_then(|p| p.texts.get(spouse).cloned())
                            .unwrap_or_default()
                    }
                    _ => return None,
                })
            }
            // The hero's guild: its name, leader, the hero's rank, members and fund.
            _ if key.starts_with("guild.") => {
                let world = self.view.sim.world_state();
                let (id, rank) = world.guild_of(&self.player.name)?;
                let g = world.guilds.get(&id)?;
                Some(match key {
                    "guild.name" => g.name.clone(),
                    "guild.leader" => g.leader.clone(),
                    "guild.rank" => rank.to_string(),
                    "guild.members" => g.members.len().to_string(),
                    "guild.fund" => g.fund.to_string(),
                    "guild.announcement" => g.announcement.clone(),
                    // The member picked: its name, level, rank and what it gave.
                    _ if key.starts_with("guild.picked.") => {
                        let name = self.picked_guild_member()?;
                        match key.strip_prefix("guild.picked.")? {
                            "name" => name.clone(),
                            "level" => world.levels.get(&name).copied().unwrap_or(0).to_string(),
                            "rank" => g.members.get(&name)?.to_string(),
                            "contribution" => {
                                g.contributions.get(&name).copied().unwrap_or(0).to_string()
                            }
                            _ => return None,
                        }
                    }
                    // The `n`th allied or enemy guild's name (`guild.ally.0`).
                    _ if key.starts_with("guild.ally.") || key.starts_with("guild.enemy.") => {
                        let (side, n) = key.strip_prefix("guild.")?.split_once('.')?;
                        let ids = if side == "ally" {
                            &g.allies
                        } else {
                            &g.enemies
                        };
                        let other = ids.get(n.parse::<usize>().ok()?)?;
                        world
                            .guilds
                            .get(other)
                            .map_or_else(String::new, |o| o.name.clone())
                    }
                    _ => return None,
                })
            }
            // The trade partner's name.
            "trade.name" => (self.state.trade.as_ref())
                .and_then(|t| self.view.sim.player(t.partner))
                .map(|p| p.name.clone()),
            // What the NPC says now, a line each.
            "dialogue.text" => (self.player.dialogue.as_ref()).map(|c| c.page.lines.join("\n")),
            // The question a typed answer answers (its option's label).
            "dialogue.question" => {
                let page = &self.player.dialogue.as_ref()?.page;
                Some(
                    page.options
                        .iter()
                        .find(|o| o.input.is_some())?
                        .label
                        .clone(),
                )
            }
            // The page's lines run together, as TQ's client joins an NPC's text packets.
            "dialogue.said" => (self.player.dialogue.as_ref()).map(|c| c.page.lines.concat()),
            _ => self.number(key).map(|n| format!("{n}")),
        }
    }

    fn item_looks(&self, key: &str) -> Vec<u32> {
        let sim = self.view.sim;
        (self.own_items(key).unwrap_or_default().into_iter())
            .map(|i| sim.item_look(i))
            .collect()
    }

    fn items(&self, key: &str) -> Vec<u32> {
        if let Some(items) = self.own_items(key) {
            return items.into_iter().map(|i| i.kind).collect();
        }
        match key {
            // The item being bought several at once.
            "purchase.item" => self.state.purchase.map(|p| p.kind).into_iter().collect(),
            // What the partner offers.
            "trade.theirs" => self
                .state
                .trade
                .as_ref()
                .map(|t| t.theirs.clone())
                .unwrap_or_default(),
            // What the open shop sells.
            "shop.items" => (self.player.shop)
                .map(|npc| self.view.sim.rules().shop(npc).to_vec())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn holding(&self) -> bool {
        self.state.held.is_some()
    }

    fn view_number(&self, group: &str) -> usize {
        (self.state.views.get(group)).map_or(0, |v| v.parse().unwrap_or(0))
    }

    fn item_tip(&self, key: &str, index: usize) -> Option<String> {
        match self.own_items(key) {
            Some(items) => Some(crate::bag::describe_item(self.view, items.get(index)?)),
            None => Some(crate::bag::describe(
                self.view,
                *self.items(key).get(index)?,
            )),
        }
    }

    fn rows(&self, key: &str) -> (Vec<String>, Option<usize>) {
        match key {
            // The hero's team: its members' names, the leader first.
            "hero.team" => {
                let names = self.player.mates.iter().map(|m| m.name.clone());
                (names.collect(), None)
            }
            // The names listed to talk to (the chat bar's list).
            "chat.targets" => (self.state.chat_bar.targets.clone(), None),
            // The hero's guild's members, or its allies or enemies (the guild tab picked).
            // The hotkey page: the groups of actions, and the picked group's actions.
            "hotkey.groups" => {
                let names = self
                    .hotkey_groups()
                    .iter()
                    .map(|g| g.name().to_owned())
                    .collect();
                (names, Some(self.view_number("hotkey_group")))
            }
            "hotkey.actions" => {
                let rows: Vec<String> = (self.hotkey_rows().iter())
                    .map(|r| r.action.label().to_owned())
                    .collect();
                let picked = (self.state.views.get("hotkey_pick")).and_then(|v| v.parse().ok());
                (rows, picked)
            }
            // The chat log's lines of the channel shown (`view:chatlog:whisper`).
            "chatlog.rows" => {
                let shown = self.state.views.get("chatlog").map_or("", String::as_str);
                let rows = (self.state.chat_log.iter())
                    .filter(|(k, _)| log_shows(shown, k))
                    .map(|(_, l)| l.clone())
                    .collect();
                (rows, None)
            }
            // The auto-path menu: the hero's map's headings and places, the places under the
            // heading open, and their tips (TQ's `\b` a line apart).
            "autorun.rows" | "autorun.tips" | "autorun.children" | "autorun.child_tips" => {
                let rows = match key {
                    "autorun.rows" | "autorun.tips" => crate::destinations::roots(self.view),
                    _ => crate::destinations::children(self.view, self.autorun_open().unwrap_or(0)),
                };
                let tips = key.ends_with("tips");
                let rows = rows.iter().map(|d| match tips {
                    true => d.mark.replace("\\b", "\n"),
                    false => d.title.clone(),
                });
                (rows.collect(), None)
            }
            // The names whose lines are never heard (the blacklist).
            "blacklist.rows" => {
                let picked = (self.state.views.get("blacklist_pick")).and_then(|v| v.parse().ok());
                (self.state.chat_filter.blacklist.clone(), picked)
            }
            // The message board's page of 8: its words (31 letters), author and time, as
            // CDlgLevWord lists them.
            "board.rows" => {
                let first = self.view_number("board_page");
                let rows = (self.board_posts().iter().skip(first).take(BOARD_PAGE))
                    .map(|p| {
                        let title: String = p.text.chars().take(31).collect();
                        format!("{title}  {}  {}", p.author, p.time)
                    })
                    .collect();
                let picked = (self.state.views.get("board_pick")).and_then(|v| v.parse().ok());
                (rows, picked)
            }
            // The hero's friends, online first, and its enemies.
            "friends.rows" | "enemies.rows" | "partners.rows" | "students.rows" => {
                let list = key.trim_end_matches(".rows");
                let picked =
                    (self.state.views.get(&format!("{list}_pick"))).and_then(|v| v.parse().ok());
                (self.contacts(list), picked)
            }
            // The guild's members on the page shown (`view:guild_member_page`), 10 a page:
            // each name and level, as CDlgKnightage lists them; the one picked marked.
            "guild.member_rows" => {
                let world = self.view.sim.world_state();
                let first = self.view_number("guild_member_page");
                let rows = (self
                    .guild_members()
                    .into_iter()
                    .skip(first)
                    .take(GUILD_PAGE))
                .map(|name| {
                    let level = world.levels.get(&name).copied().unwrap_or(0);
                    format!("{name:<19}{level}")
                })
                .collect();
                let picked = (self.state.views.get("guild_pick")).and_then(|v| v.parse().ok());
                (rows, picked)
            }
            // The answers picked by a click (not typed), as TQ's CDlgTask lists them.
            "dialogue.choices" => {
                let page = self.player.dialogue.as_ref().map(|c| &c.page);
                let picked = page.into_iter().flat_map(|p| &p.options);
                let rows = picked
                    .filter(|o| o.input.is_none())
                    .map(|o| o.label.clone());
                (rows.collect(), None)
            }
            // The answers the NPC offers.
            "dialogue.answers" => {
                let page = self.player.dialogue.as_ref().map(|c| &c.page);
                let labels = page.into_iter().flat_map(|p| &p.options);
                (labels.map(|o| o.label.clone()).collect(), None)
            }
            _ => (Vec::new(), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_game::{EntityId, Item, TradeNews};

    #[test]
    fn a_trade_follows_its_news() {
        let mut state = GuiState::default();
        let partner = EntityId(7);
        state.trade_news(partner, &TradeNews::PartnerMoney(5));
        assert!(state.trade.is_none(), "no trade before it opens");
        state.trade_news(partner, &TradeNews::Opened);
        let item = Item::new(1, 1001, None);
        state.trade_news(partner, &TradeNews::PartnerItem(item));
        state.trade_news(partner, &TradeNews::PartnerMoney(300));
        state.trade_news(partner, &TradeNews::PartnerAccepted);
        let t = state.trade.as_ref().unwrap();
        assert_eq!(
            (t.theirs.as_slice(), t.their_money, t.agreed),
            (&[1001][..], 300, true)
        );
        state.trade_news(partner, &TradeNews::Done);
        assert!(state.trade.is_none());
    }

    #[test]
    fn booth_slot_bindings_name_whose_booth_and_the_slot() {
        let slot = HeroData::booth_slot;
        assert_eq!(slot("booth.mine.coin.3", "coin"), Some(("mine", 3)));
        assert_eq!(slot("booth.theirs.price.0", "price"), Some(("theirs", 0)));
        assert_eq!(
            slot("booth.mine.0", "coin"),
            None,
            "a grid slot, not a coin"
        );
        assert_eq!(slot("booth.mine.price.x", "price"), None);
    }
}
