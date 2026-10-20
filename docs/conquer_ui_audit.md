# Conquer 5065 in-game UI audit: client source vs C3Engine remake

Spec: the 5065 client source `D:\Conquer2\Tools\eosource\cosource\MyShell` (compiled files only; DlgMain1.cpp, MenuTalk1.cpp, DlgTalk2.cpp, the Knightage* sub-dialogs and the other files excluded at MyShell.vcxproj:98 are ignored, as is code under `#if !defined(GAME_CONQUER_5065)`), `client5065\ini\GUI.ini` and `client5065\ani\Control.Ani`.
Remake: `D:\Conquer2\c3engine\games\conquer\ui\controls.ini`, with the engine in `engine\ui\src` (`gui\mod.rs`, `gui\screen.rs`, `gui_data.rs`, `lib.rs`).

Notes on line numbers:
- Client citations are `file:line` under `MyShell\` unless another folder is named (`3drole\`, `3dgamemap\`).
- controls.ini line numbers are from commit 10986dc ("The chat bar (CDlgTalk 145) with the main bar, and its channel menu (174)"). That commit landed during the audit and moved every generated draft down by about 150 lines. The hand-written part now ends at line 2201 and the generated drafts start at line 2203. A few draft line numbers in the sections below were read before the shift; the `[0-id]` lines in the table are current.
- "Status": **ok** = matches, or the client doesn't use it in 5065; **partial** = hand-written but with gaps; **draft only** = only the generated placeholder, which usually never opens and has no bindings or actions; **missing** = nothing in controls.ini.

## Summary

| Id | Dialog | Status | Mismatches |
|---|---|---|---|
| 130 | CDlgMain (main bar) | partial | 19 |
| 145 | CDlgTalk (chat bar) | partial | 16 |
| 174 | CMenuTalk (channel menu) | partial | 2 |
| 188 | CDlgMagic (normal-magic skill strip) | partial | 11 |
| 5278 | CDlgMipmap and the 5065 radar (minimap) | missing (engine substitute) | 5 |
| 304 | CDlgAutoRunToggle5065 (auto-path HUD button) | draft only | 3 |
| 325 | CDlgBigmap = mentor HUD button (IDD_DIALOG_MENTOR_HUD_5065) | draft only (wrong) | 5 |
| 328 | CDlgEquipTie5065 (item-lock button on the HUD) | draft only | 2 |
| 272 | CDlgInsHelp (instant help) | draft only | 8 |
| 295 | CDlgPigeonBanner5065 (broadcast banner line) | draft only | 6 |
| 289 | CDlgEmoneyShop5065 (floating CP-Mall button) | draft only | 2 |
| 173 | CDlgTask (NPC talk) | partial | 16 |
| 153 | CDlgMpcGoods (bag) | partial | 14 |
| 131 | CDlgQuery (status) | partial | 11 |
| 132 | CDlgQuery_Role (role page) | partial | 5 |
| 134 | CDlgQueryWeapon (weapon skills page) | partial | 6 |
| 135 | CDlgEquip (equipment page) | partial | 7 |
| 163 | CDlgSkill (skills page of 131) | missing | 1 |
| 286 | CDlgBattlePower | draft only | 5 |
| 287 | CDlgBattlePowerDetail | draft only | 4 |
| 152 | CDlgNpcBuy (NPC shop) | partial | 14 |
| 161 | CDlgDepot (warehouse) | partial | 14 |
| 141 | CDlgGroup (team) | partial | 13 |
| 268 | CMenuGroup (team options) | partial | 3 |
| 158 | CDlgConfirm (generic yes/no prompt) | draft only | 14 |
| 137 | CDlgTrade (trade) | partial | 21 |
| 140 | CDlgAct (emote bar) | draft only | 6 |
| 191 | CDlgPk (PK mode picker) | draft only | 9 |
| 189 | CDlgXp (XP / revive / land / abort-transform strip) | draft only | 6 |
| 214 | CDlgKongfu (Kongfu skill strip) | draft only | 5 |
| 133 | CDlgKnightage (guild) | partial | 20 |
| 257 | CDlgFacState (guild member info) | draft only | 4 |
| 160 | CDlgFriend (friends) | draft only | 12 |
| 269 | CDlgEnemy (enemies) | draft only | 8 |
| 209 | CDlgFrdState (friend info) | draft only | 7 |
| 270 | CDlgEnmState (enemy info) | draft only | 6 |
| 138 | CDlgSetup (options) | draft only (unreachable) | 17 |
| 231 | CDlgSetup_Hotkey (IDD_DIALOG_HOTKEY) | draft only | 8 |
| 255 | CDlgOther (setup "Other": furniture) | draft only | 4 |
| 302 | CDlgSetup_Graph (low-FPS restart prompt) | draft only | 5 |
| 274 | CDlgTalkAct (emotion icon picker) | draft only | 6 |
| 148 | CDlgListBox (whisper-name history) | draft only | 3 |
| 139 | CDlgChat (chat options: filters and colours) | draft only | 11 |
| 239 | CDlgBlack (blacklist) | draft only | 8 |
| 240 | CDlgLevWord (message board) | draft only | 10 |
| 5326 | CDlgNorMsg (chat-log line handles) | missing | 3 |
| 218 | CDlgLog (chat history viewer) | missing | all |
| 2232 / 2233 / 2234 | native private chat windows and tray | missing | 5 |
| 294 | CDlgBroadcast5065 (pigeon broadcast board) | draft only | 9 |
| 296 | CDlgPigeonPrompt5065 (broadcast prompts) | draft only | 5 |
| 293 | CDlgOfflineMessage5065 (whisper to an offline friend) | draft only | 4 |
| 229 | CDlgEmbed (gem sockets) | partial | 13 |
| 275 | CDlgItemCompose5065 (+N composing) | partial | 12 |
| 211 | CDlgImprove (raise quality / level) | draft only | 6 |
| 278 | CDlgOven (refine / fuse) | draft only | 6 |
| 105 | CDlgEnchant | draft only | 4 |
| 298 | CDlgBrokenRepair5065 | draft only | 5 |
| 212 | CDlgNpcFace (change avatar) | draft only | 6 |
| 233 | CDlgMpcBooth (own booth) | draft only | 11 |
| 234 | CDlgNpcBooth (another player's booth) | draft only | 8 |
| 235 | CDlgHawk (booth cry-out) | draft only | 3 |
| 232 | CDlgMsgSend (leave a word) | draft only | 6 |
| 238 | CDlgMsgReceive (received words) | draft only | 6 |
| 290 | CDlgItemPurchase5065 (+292 CDlgBuyMessage5065) (CP buy amount) | draft only | 10 |
| 330 | CDlgEmoneyWithdraw5065 (CP withdraw button) | draft only | 2 |
| 277 | CDlgPortableChest5065 (portable chest) | missing | 6 |
| 329 | CDlgItemLock5065 (item lock) | draft only | 7 |
| 258 | CDlgNpcEquip (another player's equipment) | draft only | 10 |
| 241 | CDlgEffigy (statue placement) | draft only | 9 |
| 242 | CDlgLay (place an NPC) | draft only | 6 |
| 262 | CDlgDice | draft only | 9 |
| 267 | CDlgDiceList | draft only | 3 |
| 284 | CDlgOfflineTrainingResult | draft only | 8 |
| 285 | CDlgOfflineTrainingConfirm | draft only | 5 |
| 303 | CDlgAutoRun5065 (auto-path destinations) | draft only | 7 |
| 334 | CDlgFurnishment5065 (item-bind list) | draft only | 5 |
| 380 | CDlgQuestList5065 / 381 CDlgQuestTrack5065 | draft only (non-native additions) | 7 |
| 215 | CDlgHelp | draft only | 2 |
| 312 | CDlgSchoolList (mentor) | draft only | 9 |
| 313 / 314 / 316 | mentor state pages, 315 grand-student list | draft only | 4 |
| 317 | CDlgMentorNotice5065 / 318 CDlgMentorConfirm5065 | draft only | 4 |
| 319 / 320 / 322 / 323 / 324 | mentor contribution pages | missing | all |
| 326 | CDlgBusiness (trade partners) | draft only | 7 |
| 327 | CDlgBusinessState | draft only | 4 |
| 331 | CDlgBusinessTip5065 | draft only | 4 |
| 332 | CDlgBusinessConfirm | draft only | 4 |
| 333 | CDlgBusinessShare5065 | draft only | 3 |
| 307 | CDlgNobilityMain | draft only | 6 |
| 308 | CDlgNobilityConfirm | draft only | 4 |
| 309 | CDlgNobilityRanking | draft only | 5 |
| 310 | CDlgNobilityCompact | draft only | 3 |
| 365 | CDlgEmotion | ok (dead in 5065) | 0 |

Current `[0-id]` lines in controls.ini:
- **Hand-written:** 130:6, 153:152, 131:872, 152:1382, 173:1417, 161:1485, 137:1541, 141:1605, 268:1746, 133:1769, 275:1895, 229:1942, 188:1984, 145:2056, 174:2170.
- **Drafts:** 140:2206, 303:2268, 304:2280, 325:2287, 239:2301, 294:2334, 298:2397, 327:2418, 333:2462, 326:2478, 332:2514, 331:2543, 139:2558, 158:2676, 262:2722, 267:2830, 241:2838, 289:2904, 330:2912, 105:2920, 269:2949, 270:2981, 257:3029, 209:3051, 160:3099, 235:3135, 277:3152, 215:3169, 211:3173, 272:3202, 334:3237, 329:3268, 328:3294, 290:3302, 214:3367, 242:3409, 240:3444, 148:3521, 317:3529, 318:3540, 233:3555, 238:3694, 232:3735, 307:3768, 308:3826, 309:3846, 310:3912, 234:3920, 258:4031, 212:4089, 293:4134, 285:4150, 284:4172, 255:4230, 278:4257, 295:4295, 296:4306, 191:4333, 380:4341, 381:4370, 313:4391, 314:4443, 316:4501, 315:4538, 312:4553, 138:4612, 302:4703, 231:4719, 305:4759, 287:4793, 286:4810, 274:4854, 195:4876, 189:4880.
- **Missing:** 163, 218, 319, 320, 322, 323, 324, 5326, 2232, 2233, 2234, and 292 (inside 290). 277 is a draft of the wrong class.

## Most important problems

1. **Most dialogs can never open.** About 70 dialogs are generated drafts. They have no `open=always`, no `when`, no `window` and no action that opens them. Nothing wires the client's server-message openings to them: confirm 158, trade/team/guild requests, XP 189, booths, mentor, nobility, offline training, and more.
2. **The main bar's buttons lead nowhere useful:**
   - 1006 (message board 240) opens the engine's chat line.
   - 1008 (Setup 138) opens the engine's egui menu.
   - 1009, 1013 and 1026 open drafts (140 actions, 191 PK, 160 friends) that have no working controls.
   - The XP orb, the stamina bars and the F1-F10 quick bar are missing.
3. **There is no generic yes/no prompt (158).** Trade, team and guild requests only print a chat line, or auto-join, so they cannot be accepted or refused the way the client does it. Drop-money, donate, gem take-out and leave-guild have no confirm either.
4. **Item handling is click-only.** There is no drag-and-drop, no right-click use or buy, no tooltips, no +N or quality overlays, no paging, and no repair mode. Bag clicks fill the compose and gem workbenches strictly in order, with no type checks, and the bag doesn't open with them. Shop, warehouse and trade lose everything past their first page.
5. **The engine has no client-style window model:**
   - It uses a rect-overlap rule instead of the client's left/middle/right owner logic (CloseCurDlg). Child dialogs close their owners (209 over 160, 257 over 133), and HUD strips close each other (140, 188, 191).
   - Esc doesn't close dialogs.
   - Buttons have no disabled or latched states, and no slide-in or Help.
   - Several always-visible HUD dialogs are not shown: 304, 325, 328, 272, 295.
   - Its own egui widgets still draw over or instead of the client's: the skill bar, the radar circle, the chat log and the quest windows.

## Dialogs (most visible first)

### 130 CDlgMain (main bar) - partial, 19 mismatches
- Client: created MyShellDlg.cpp:5112 (IDD_MAIN_DIALOG); always shown; GUI.ini [0-130] 0,627 1024x141. OnInitDialog (DlgMain.cpp:154-200) inits: GoodBox 1021 (quick bar, CMyGrid 1x10, moved to 91,101 409x40), life 1501 Progress40 (MY_PROGRESS_V, show mode 1, SetWidthAndHeight 36,74,86), mana 1502 Progress41 (34,74,34), XP 1018 Progress42 (92x92), stamina 1025 Progress46 (8,70), extra stamina 1027 Progress47 (8,35), exp 1019 Progress44 (MY_PROGRESS_HLEFT); buttons 1503 Button40 status, 1026 Button41 friends, 1007 Button42 bag, 1008 Button43 setup, 1009 Button44 actions, 1010 Button45 trade, 1011 Button46 team, 1013 Button47 PK, 1024 Check62 guild, 1006 Button410 message board; checks 1012 Check40 run, 1015 Check43 map, 1023 Check46 screen move, 1016 Button411 view equipment; 1022 skill box Magic0. It then creates nearly every in-game dialog (DlgMain.cpp:206-604). Show (DlgMain.cpp:669-866) draws Progress45 underlay, life/mana fills (value + forecast), stamina (x1.5 while Blessed) and extra stamina, the Dialog4 bezel in four ShowEx pieces, XP, exp at y-1, quick bar, buttons and checks (Check43 frame 0 when the radar is open, 1 when rolled), then 145, 294, the item-lock owners and ShowChildDlg (all child dialogs, DlgMain.cpp:868-1139); XP flashes Progress42 mode 2 for 500 ms after an XP change (849-864). Hover on exp/run/map/screen-move/view-equipment/XP/MP/HP posts ON_MSGDLG_STATIC (DlgMain.cpp:1364-1389) and the shell draws a Dialog21 tooltip ("cur/max" for meters, StrRes 10070/10072/10073-10074/10370, detailed exp overlay) (MyShellDlg.cpp:5667-5745). Handlers: OnMainQuery 1149, OnMainFriend 1183, OnMainGood 1205, OnMainSetup 1220, OnMainChat (trade) 1246, OnMainGroup 1258, OnMainLeaveWord 1272, OnMainAct 1284, OnMainPk 1293, OnMainMap 1302, OnMainScreenMove 1311, OnMainNpcEquip 1397, OnMainSkillBox -> OnMainMagic 1405/1800, OnMainFaction 1606.
- Remake: hand-written (controls.ini:5-149), open=always. Draw order and the Progress45 / fills / Dialog4 bezel pieces match DlgMain.cpp:741-782. Opens like the client: yes.
- Mismatches:
  - 1006 Button410 runs action=chat (engine chat line); the client toggles the message board 240 (OnMainLeaveWord, DlgMain.cpp:1272-1282).
  - 1008 Button43 runs action=menu (engine egui Options); the client toggles CDlgSetup 138 after closing emotion/owner/status/friends (DlgMain.cpp:1220-1242).
  - 1009 / 1013 / 1026 toggle 140 / 191 / 160, which are drafts with no working controls (DlgMain.cpp:1183-1203, 1284-1300).
  - XP orb 1018 Progress42 (GUI.ini 0,50 92x86) missing, with its 500 ms mode-2 flash (DlgMain.cpp:173-175, 790-791, 849-864); no XP binding.
  - Stamina 1025 Progress46 (GUI.ini 42,58 12x78) missing, including the x1.5 capacity while Blessed (DlgMain.cpp:176-178, 703-709, 755-756); no stamina binding.
  - Extra stamina 1027 Progress47 (shown while Blessed above 99) missing (DlgMain.cpp:179-181, 757-762).
  - Quick bar 1021 (1x10 grid at 91,101 409x40) missing: items/skills/emotes dragged in, F1-F10 use, ItemUse effect, suspicious-item effect (DlgMain.cpp:162-164, 804-806, 1732-1797; MyShellDlg.cpp:4805-4818).
  - 1022 skill box only toggles 188; it never shows the selected right-click skill (MagicSkillType<id> / XpSkillType<id> put in by ON_MAGIC_CHOOSE / ON_XP_CHOOSE) and there is no right-click casting of it (MyShellDlg.cpp:4194-4235).
  - 1015 Check43 has no value or action; the client toggles the radar with a transition and draws frame 0/1 from radar visibility (DlgMain.cpp:818-822, 1302-1309).
  - 1023 Check46 has no value or action; the client posts ON_MAIN_SCREENMOVE (free camera / ResetViewPos) (DlgMain.cpp:1311-1316; MyShellDlg.cpp:4135-4138).
  - 1016 Button411 has no value or action; the client arms CMD_NPCEQUIP so the next player clicked opens 258 (DlgMain.cpp:1397-1403; MyShellDlg.cpp:2701-2706, 2323-2331).
  - 1013 is always Button47; the client swaps Button47/49/48/412 with the PK mode (MyShellDlg.cpp:9048-9064).
  - No hover tooltips on exp, run, map, screen move, view equipment, XP, MP, HP (DlgMain.cpp:1364-1389; MyShellDlg.cpp:5667-5745).
  - Life orb ignores the client's show modes: full-circle frame 2 (width 86) until the hero has mana, then frame 0 at 36x74 (mana 34x74); the remake fills the GUI.ini 40x86 rect with frame 0 always (DlgMain.cpp:166-172, 730-735; MyProgress.cpp:139-195).
  - Life/mana forecast (value vs current, two-pass draw) missing (DlgMain.cpp:749-754; MyProgress.cpp:105-136).
  - Exp is drawn at the GUI.ini y; the client draws it 1 px up (comment at controls.ini:65 says so but no offset is set) (DlgMain.cpp:796).
  - Exp skin Progress44 is not in client5065/ani/Control.Ani (only Control1.Ani:1610 / Control2.Ani:1669) and both the client (GameSetUp.ini InterfaceRecord=0) and the remake (module.rs ui.skin) read Control.Ani - check in game whether 1019 draws at all (DlgMain.cpp:182).
  - 1010 trade: no dead-hero refusal (StrRes 10712) and no 10065 prompt; the engine prints "Click a player (trade)." (DlgMain.cpp:1246-1256; MyShellDlg.cpp:3799-3802).
  - Button sounds differ: the client plays BtnClick.wav on most buttons, DropMoney.wav on 1010 and nothing on 1011/1024; the engine plays DlgOpen/DlgClose on every toggle (DlgMain.cpp:1151, 1248, 1258-1270, 1606-1628).
- Note: CDlgMain also keeps several always-visible HUD dialogs that the remake does not show: 304 auto-path button (DlgMain.cpp:401-403), 325 mentor button (357-361), 328 item-lock button (2712-2715, 840), 272 instant help (355, 1105), 295 pigeon banner (MyShellDlg.cpp:5113, 1429-1430). The engine instead adds its own egui hint line, status row, team list, radar, skill bar and quest tracker on top (lib.rs show()).

### 145 CDlgTalk (chat bar) - partial, 16 mismatches
- Client: created and shown DlgMain.cpp:393-394 (never hidden in game), drawn every frame from CDlgMain::Show (DlgMain.cpp:835-836). Enter anywhere focuses the line unless another edit has focus (MyShellDlg.cpp:4851-4866; CDlgMain::OnShellEnter DlgMain.cpp:1727-1730). Show (DlgTalk.cpp:329-374): Dialog5 frames 0-3 at x+0/256/512/768, y-2; then data line 1542, name 1544, mode image 1094, Clear 1092, Add 1545, List 1185, Send 1543, Log 1095, Action 1096; then children MenuTalk 174, ListBox 148, TalkAct 274; in broadcast mode the tip StrRes 11034 above 1094. Controls (OnInitDialog DlgTalk.cpp:246-300; GUI.ini [0-145] 82,697 720x30):
  - 1542 edit (CMyCombo, 230,1 327x21): white, limit 88 (DlgTalk.cpp:262-264), inline #NN emoticons (MyCombo.cpp:98), Up/Down steps a 20-line history (MyCombo.cpp:200-208; DlgTalk.cpp:167-203, 531-553), Enter sends (DlgTalk.cpp:161-165).
  - 1544 name (37,1 93x21): cyan, limit 15, offset (4,2) (DlgTalk.cpp:266-268); red when in the target list, white otherwise (DlgTalk.cpp:580-628).
  - 1094 mode image (CMyImage, Button50); click toggles 174 (DlgTalk.cpp:270, 557-563); SetTalkMode swaps Button50 talk / 51 whisper / 52 team / 53 guild / 54 friend / 55 service / ad_channel broadcast (DlgTalk.cpp:638-683).
  - 1092 Clear Button514: ClearMsg + CMD_PIGEON_BANNER_RESET_5065 (DlgTalk.cpp:787-791; MyShellDlg.cpp:8300-8302).
  - 1545 Add Button57/58/59 by state: add the name to 148, or confirm delete (DlgTalk.cpp:630-636, 809-825; MyShellDlg.cpp:4025-4030, 4075-4077).
  - 1185 List Button510: toggles 148 (DlgTalk.cpp:793-807).
  - 1543 Send Button511.
  - 1095 Log Button512: ON_TALK_OPTIONS_5065 toggles CDlgChat 139 (DlgTalk.cpp:827-838; MyShellDlg.cpp:4081-4100).
  - 1096 Action Button513: toggles 274 and hides 191 (DlgTalk.cpp:840-851; MyShellDlg.cpp:2678-2681).
  - Send (DlgTalk.cpp:391-554): strips line breaks, `/` commands via DispatchAndGuardChatCommand5065, 500 ms flood guard (StrRes 10044); talk 2000, whisper 2001, team 2003, guild 2004, friend 2009, service 2014 (20 s cooldown, StrRes 10444); broadcast needs level 50, 5 CPs, 80 bytes and opens prompt 296 (DlgTalk.cpp:493-526); the name is sent as receiver on every channel (431-437).
  - Keys/mouse: Ctrl+Up/Down cycles channel, Ctrl+Left/Right the 5 recent (channel, name) pairs (MyShellDlg.cpp:4738-4790, 4950-4976; DlgTalk.cpp:692-785); Shift+click a player sets the name (MyShellDlg.cpp:2262-2268); Alt+click a log name captures it (2212-2217); friend/guild/mentor/business/leave-word/pigeon "chat" routes set name and mode (2682-2806, 8320-8351, 10816-10818). Target list and channel colours saved (DlgMain.cpp:2098-2113, 2137, 2256-2268, 2325-2341).
- Remake: hand-written since commit 10986dc (controls.ini:2052-2168): open=always; Dialog5 as four images 9500-9503 at y=-2 (right); 1542 edit on chat.text, action=chat_send; 1544 edit chat.name cyan; 1094 + 9510-9514 six buttons switched by view.chat.* that toggle:174; 1092 chat_clear; 1543 chat_send; 1096 toggle:274. The message log itself is still the engine's egui chat (chat.rs). Opens like the client: yes.
- Mismatches:
  - Broadcast (A) mode missing: no ad_channel skin, tip 11034, placeholder 11023, level/CP/length checks or 296 prompt (DlgTalk.cpp:357-373, 493-526, 645-649, 676-679).
  - Friend and service can be picked but chat_send refuses them; the client sends 2009 and 2014 (DlgTalk.cpp:466-492).
  - 1185 has no action; should toggle 148 (DlgTalk.cpp:793-807).
  - 148 CDlgListBox (recent targets) still a draft: list 1099 unbound (yellow), Dialog24 at (-3,-4), row click fills the name, hides on mouse-leave, arrows [148-1]/[148-2], no persistence (DlgListBox.cpp:54-58, 121; DlgTalk.cpp:219-231, 376-389).
  - 1545 no action and fixed Button57; should switch 57/58/59, add or confirm-delete (DlgTalk.cpp:580-636, 809-825).
  - 1544 colour does not follow state (red listed / white) and no 15-char limit (DlgTalk.cpp:267, 597-627).
  - 1095 no action; should toggle 139 (DlgTalk.cpp:827-838).
  - 1096 does not also hide 191 (MyShellDlg.cpp:2678-2681).
  - chat_clear clears only the egui lines; the client clears every area (system, GM) and resets banner 295 (DlgTalk.cpp:787-791).
  - 1542 has no 88-char limit and no inline #NN emoticons (DlgTalk.cpp:264; MyCombo.cpp:98).
  - No Up/Down history of 20 sent lines (MyCombo.cpp:200-208; DlgTalk.cpp:167-203).
  - No Ctrl+Up/Down channel cycling or Ctrl+Left/Right recent targets (MyShellDlg.cpp:4776-4790; DlgTalk.cpp:692-785).
  - No 500 ms flood guard or 20 s service cooldown (DlgTalk.cpp:418-424, 481-485).
  - Shift+click player, Alt+click log name and the friend/guild/business/leave-word/pigeon chat routes do not fill 1544 (MyShellDlg.cpp:2212-2217, 2262-2268, 2682-2806).
  - 1094 is one CMyImage with ChangeImage in the client (DlgTalk.cpp:270, 296); the six when-switched buttons work but show frame 1 only while pressed; clicking the bar does not focus 1542 (DlgTalk.cpp:574-578).
  - The log under the bar is the engine's egui box (7 lines, backing boxes, one area), not g_objGameMsg's three areas (see "Message rendering" below) (MyShellDlg.cpp:1444; GameMsg.cpp:1469-1589).

### 174 CMenuTalk (channel menu) - partial, 2 mismatches
- Client: created by CDlgTalk (DlgTalk.cpp:280-282), moved over 1094 (314-326); draws its 7 buttons Button50-55 and ad_channel (MenuTalk.cpp:130-143); a click posts MENU_TALK_CLICK and hides (150-212).
- Remake: hand-written (controls.ini:2170-2201), 1182-1184, 1546-1548; GUI.ini [0-174] 254,503 equals the client's MoveWindow.
- Mismatches:
  - [174-1549] ad_channel (broadcast) missing (MenuTalk.cpp:114, 205-212; GUI.ini [174-1549]).
  - Friend and service do nothing on send (DlgTalk.cpp:466-492).

### Message rendering (g_objGameMsg; what 145 sits under)
- Client: drawn by the shell (MyShellDlg.cpp:1444, g_objGameMsg.Show(Alt down)). Three areas (GameMsg.cpp:1469-1589): system 2005 top-left, 3 lines, 15 s, repeat flashes red (1808-1823); GM 2011 at x=100 mid-screen, 15 s; normal bottom-left from screenH-130 upward, wrapped to screen width, slider line count; ghost channel only for ghosts. Line format: channel prefix (STRING_PREFIX_*), sender, emotion, "to" receiver, "says", text, timestamp (GameMsg.cpp:962-1013); styles scroll / flash / blast; per-channel colours (2193-2224); Alt makes names clickable (tip 100130); incoming lines filtered by blacklist, channel on/off and friend-only (MsgTalk.cpp:1124-1131); speaker gets an overhead line (1170-1176); lines logged for CDlgLog 218.
- Remake: engine egui chat log (chat.rs): 7 lines with dark backing boxes, one area, no prefixes/timestamps/colours per channel, no system/GM areas.

### 188 CDlgMagic (normal-magic skill strip) - partial, 11 mismatches
- Client: created hidden DlgMain.cpp:599-600; toggled by the HUD skill box 130-1022 -> OnMainSkillBox -> OnMainMagic (DlgMain.cpp:1405-1408, 1800-1838): acts only when GetMagicAmount()>0, hides XP 189 when opening, also toggles Kongfu 214, moves the strip to x=1024-n*50-18, y=768-123, w=n*50+18 (n=min(count,10)). Re-initialised on skill changes (MyShellDlg.cpp:9344-9347); hidden on death (DlgMain.cpp:2389-2391) and booth close (684-686). Draws 10 Magic.Ani MagicSkillType<type> icons (DlgMagic.cpp:125-134, 154-239); Button173/174 paging only with more than 10 skills (137-140, 170-190).
- Remake: hand-written (controls.ini:1984-2050), toggled by 130-1022; the id->index mapping (1592, 1584, 1585, 1587, 1588, 1593, 1586, 1589, 1590, 1591 -> skill.0..9) matches the client. Opens like the client: partly (no count gate, no Kongfu/XP handling, fixed width).
- Mismatches:
  - A click runs cast:N and casts; the client only selects the skill into the HUD skill box as the right-click skill (ON_MAGIC_CHOOSE, MyShellDlg.cpp:4194-4211; DlgMagic.cpp:241-299).
  - The strip stays open after a pick; the client hides 188 and 214 (DlgMagic.cpp:244; MyShellDlg.cpp:4198-4202).
  - Fixed x=506 w=518; the client right-aligns and sizes by skill count, so with fewer skills the remake leaves a gap on the right (DlgMain.cpp:1816-1818).
  - Paging 188-1275 Button173 / 188-1276 Button174 missing; skills past 10 unreachable (DlgMagic.cpp:137-140, 301-413).
  - skill.N lists every hero skill; the client uses GetMagicByIndex, skips dwScreenRepresent==3 and transform magics while transformed (DlgMagic.cpp:193-197); Kongfu skills belong in 214.
  - Disabled skills get no Image0 cover and clicks are not blocked (DlgMagic.cpp:200; MyImage.cpp:288-297, 482-483).
  - No hover tooltip (ON_MSGDLG_IMAGE) and no drag to the quick bar (MyImage.cpp:484-525).
  - The opener toggles only 188; the client also toggles 214, hides 189 and does nothing with 0 skills (DlgMain.cpp:1806-1838).
  - Overlap heuristic: opening 140 (283,628 458x56) or 191 closes 188; independent in the client (DlgMain.cpp:1284-1300).
  - The engine's egui skill bar is always drawn too, duplicating this strip (lib.rs hud::skill_bar) (DlgMain.cpp:1121-1122).
  - Not hidden on death or booth close (DlgMain.cpp:684-686, 2389-2391).

### 5278 CDlgMipmap and the 5065 radar (minimap) - missing (engine substitute), 5 mismatches
- Client: 5278 created DlgMain.cpp:576-587 but hidden in 5065 (its Show returns early, DlgMipmap.cpp:146-165; it only sets the radar's collapse point, 129-144). The minimap is CGameMap::Show -> m_objRadar.Show (3DGameMap.cpp:303-305) in a 170x128 top-right rectangle (839-841): ani/MiniMap.ani section <mapid>, 4 tile frames at alpha 0x99, modes crop/overview/full (Radar.cpp:180-224, 290-415); marks MiniMap.ani hero plus common.ani PlayerFlag/TeamLeader/Tutor/Student/Friend (453-520). Check43 130-1015 -> OnMainMap -> SetTransition(1/2) animated grow/shrink (DlgMain.cpp:1302-1309; Radar.cpp:87-178); Check43 frame 0 open / 1 rolled (DlgMain.cpp:818-822); MapModeRecord restored (2213-2219). Radar buttons Button01 (overview/crop) and Button02 (full/crop) while visible (MyShellDlg.cpp:5078-5085, 10433-10472).
- Remake: no 5278 (correct); the engine draws its own egui radar circle (radar.rs).
- Mismatches:
  - Egui circle instead of the 170x128 MiniMap.ani crop at 0x99 alpha (3DGameMap.cpp:839-841; Radar.cpp:290-415).
  - No overview/full modes, no Button01/Button02 (MyShellDlg.cpp:10433-10472).
  - 130-1015 Check43 has no action or value: no toggle, no transition, frame does not follow visibility (DlgMain.cpp:818-822, 1302-1309).
  - Marks do not use MiniMap.ani/common.ani icons (Radar.cpp:453-520).
  - No MapModeRecord persistence, no map hotkey (action 61) (DlgMain.cpp:2213-2219).

### 304 CDlgAutoRunToggle5065 (auto-path HUD button) - draft only, 3 mismatches
- Client: created and shown permanently DlgMain.cpp:401-403, rendered 873-874; path_auto (4 frames); click plays BtnClick and toggles 303.
- Remake: draft (controls.ini:2280).
- Mismatches:
  - No open=always (DlgMain.cpp:403).
  - 1540 has no ani (path_auto).
  - No action (toggle:303) (DlgAutoRunToggle5065.cpp OnToggleAutoRun).

### 325 CDlgBigmap = mentor HUD button (IDD_DIALOG_MENTOR_HUD_5065) - draft only (wrong), 5 mismatches
- Client: created and shown at DlgMain.cpp:357-361 (always visible). Draws only button 1571 btn-dssy at GUI (165,639,53,53) with a Dialog21 tooltip StrRes 100220 (DlgBigmap.cpp:101-107, 125-130); click -> CMD_MENTOR_SWITCH_CONTRIBUTION_5065/3 -> 319 (DlgBigmap.cpp:63-68).
- Remake: draft.
- Mismatches:
  - No open=always, never shown (DlgMain.cpp:361).
  - ani=Dialog531 is the non-5065 big-map frame and does not exist in the 5065 Control.Ani.
  - [325-1180] Button532 is non-5065; section missing and GUI.ini does not place it.
  - 1571 has no action (DlgBigmap.cpp:63-68).
  - Tooltip missing (DlgBigmap.cpp:125-130).

### 328 CDlgEquipTie5065 (item-lock button on the HUD) - draft only, 2 mismatches
- Client: created and shown permanently (DlgMain.cpp:2712-2715), drawn every frame at GUI.ini 223,654 (DlgMain.cpp:840, 2719-2725); EquipTie_TieLockBtn 2186 toggles 329 (DlgItemLock5065.cpp:656-667).
- Remake: draft (controls.ini:3143-3149).
- Mismatches:
  - [0-328] lacks open=always, so this always-visible HUD button never shows (DlgMain.cpp:2712-2715).
  - 2186 has no action (toggle 329 gated on the right slot, show the bag) (DlgItemLock5065.cpp:656-667).

### 272 CDlgInsHelp (instant help) - draft only, 8 mismatches
- Client: created DlgMain.cpp:355, rendered every frame (1105). Compact Button490 15x32 at (110, 768-105), shown at login and every level-up (MyShellDlg.cpp:9159, 10381-10400), flashing frames 0/2 at milestone levels; click expands to a centred 494x288 Dialog49 with help text, Tip/PageUp/PageDown/More and two close buttons that collapse it (DlgInsHelp.cpp Show/OnExpandHelp/CollapseToCompact5065).
- Remake: draft (controls.ini:3202); no open=always.
- Mismatches:
  - Not always visible (MyShellDlg.cpp:10399).
  - Compact/expanded states not representable; GUI [0-272] is 15x32 so 1430 at (465,24) falls outside.
  - Button490 flashing missing (frame 2 is hover in the engine).
  - 1432 Button490 no expand action (MyShellDlg.cpp:2891-2896).
  - 1430/1433 toggle:272 hide the whole dialog; the client collapses to the compact button.
  - 1434 Tip (Button492), 1435/1436 paging no action.
  - 1431 More (Button491) no action (URL from oem.ini / StrRes 10367).
  - Help text not drawn; no binding.

### 295 CDlgPigeonBanner5065 (broadcast banner line) - draft only, 6 mismatches
- Client: created MyShellDlg.cpp:5113; shown every frame above the line slider (MovePigeonBannerFromLineSlider5065, 10415-10431; drawn 1429-1430). ad_query 1515, latest pigeon line 1514 (name 0xaaffaa, separator 100018, text in the private colour), tip 11042, Ctrl+click sets the whisper target, "Pigeon" effect per line (DlgPigeonBanner5065.cpp:184-202, 213-239, 259-331); fed by channel 2500 (MsgTalk.cpp:1117-1123).
- Remake: draft.
- Mismatches:
  - No open=always (MyShellDlg.cpp:5113).
  - Place should follow the line slider, not GUI.ini 0,0 (MyShellDlg.cpp:10415-10431).
  - 1515 has no toggle:294 (DlgPigeonBanner5065.cpp:173-176).
  - 1514 unbound, no colours.
  - Ctrl+click target, tip and flash missing.
  - "Pigeon" effect missing.

### 289 CDlgEmoneyShop5065 (floating CP-Mall button) - draft only, 2 mismatches
- Client: created DlgMain.cpp:492/499; ShowRenderer (DlgMain.cpp:1094-1095) draws only the EmoneyShopON button; shown/hidden by CMD_EMONEY_SHOP_SHOW/HIDE_5065 (MyShellDlg.cpp:8261-8268); RefreshEmoneyShopVisibility5065 hides it below level 70 without a second rebirth and with 0 CPs (3drole/EmoneyVisibility5065.cpp:24-33). GUI.ini 132,653 40x40.
- Remake: draft (controls.ini:2753-2758); section right, no condition, no action.
- Mismatches:
  - Never shows; no CP binding and `when` cannot test level >= 70 (EmoneyVisibility5065.cpp:24-33).
  - 1510 has no action; the client plays BtnClick.wav and posts CMD_EMONEY_SHOP_OPEN_5065 (closes an NPC shop, opens shop 2888) (DlgEmoney5065.cpp:114-120; MyShellDlg.cpp:8248-8260).

### 173 CDlgTask (NPC talk) - partial, 16 mismatches
- Client: created MyShellDlg.cpp:5090, drawn 1576-1577. Opens on MsgTaskDialog action 100 (MSGDIALOG_CREATE) -> CMD_TASKDIALOGOPEN (3drole/Network/MsgTaskDialog.cpp:138-154; MyShellDlg.cpp:9161-9163). Hides on an answer click (DlgTask.cpp:579-587), the answer button (589-598), close (600-604), walking by ground click (MyShellDlg.cpp:2187-2190), ON_DLGTASK_CLOSE when the guild opens (DlgMain.cpp:1616 -> MyShellDlg.cpp:3542-3545). Show (DlgTask.cpp:449-554): Dialog23's 544x200 corner stretched to 544 x m_nDlgHeight; NpcFace.Ani Face<n> at (10,30) scaled 64x64; text packets joined, yellow 0xffff00 from (84,30) wrapped at 420; answers white, red under the mouse, two columns at x=20 and x=282, 210 wide, wrapped, starting at max(text bottom, 104) (371-401); Button5 close 1180. Typed answer: edit at x=40 or 282 on an Edit230 background (180x18), yellow title 18 px above, Button230 1549 beside it (403-445, 535-550); max length dwData&0xff, password when (dwData>>8)&0xff (429-440).
- Remake: hand-written (controls.ini:1414-1479), when=hero.talking (matches the server-driven open); suppresses the egui dialogue.
- Mismatches:
  - Text wraps at 440 (w=440); the client wraps at 420 (DlgTask.h:17; DlgTask.cpp:366-369).
  - dialogue.text joins lines with "\n" (gui_data.rs:250); the client concatenates packets with no separator (DlgTask.cpp:265-279).
  - Answer columns at 84/304, 220 wide; the client uses 20/282, 210 wide (DlgTask.cpp:384-386).
  - Answers start 4 px under the text; the client starts at max(text bottom, 104), clearing the face (DlgTask.cpp:373-375).
  - Rows are single 18 px lines that never wrap; the client wraps each within 210 px (DlgTask.cpp:376-383, 505-531).
  - Hover draws a translucent box, text stays white; the client turns the text red (DlgTask.cpp:516-525).
  - The typed answer's label is a clickable list row; the client draws it as a yellow title above the edit (DlgTask.cpp:536-547).
  - [173-1549] Button230 answer button missing; the remake sends on row click (DlgTask.cpp:238, 423-425, 441, 549, 589-598).
  - Edit at x=84 below the list; the client puts it in the answer grid (x=40/282) and gives it 44 px (DlgTask.cpp:413-418, 442-443).
  - No Edit230 background behind the edit (DlgTask.cpp:418-428).
  - No max length / password mode from the server data (DlgTask.cpp:429-440).
  - An empty typed answer is still sent (lib.rs:80-84); the client ignores it (DlgTask.cpp:591).
  - fit=104 forces 104 px on every page; the client only when there are answers or an edit (DlgTask.cpp:335, 373, 405).
  - x=240 fixed centres on the 1024 base; the client centres on the real screen width (DlgTask.cpp:336).
  - Esc closes the dialogue (engine back(), lib.rs:519-520); the client swallows IDCANCEL (DlgTask.cpp:206-214).
  - No 8-answer limit (DlgTask.h:49; DlgTask.cpp:298-299).

### 153 CDlgMpcGoods (bag) - partial, 14 mismatches
- Client: created DlgMain.cpp:374. Toggled by main-bar Button42 (OnMainGood, DlgMain.cpp:1205-1217) and hotkey 44 (MyShellDlg.cpp:10253); always shown with the NPC shop (DlgMain.cpp:1652-1668), warehouse (1670-1682), oven/enchant/compose (1687-1725), improve/embed/booth (2408-2496), item lock (2728-2765), auction (3139-3176) and trade (MyShellDlg.cpp:8846-8850). Hidden by its close button (OnMpcGoodsClose, MyShellDlg.cpp:9477), shop close (3784-3797), trade close (3807-3810), CloseCurDlg (DlgMain.cpp:1534-1552), dice (2583), mentor contribution (2808, 2825), battle-power detail (MyShellDlg.cpp:8594-8599). Slides in from the right (DlgMpcGoods.cpp:76-97, 172-187). Draws Dialog12; grid 1126 8 rows x 5 cols at 26,28 (215x344); money 1127 and CPs 1129 (white, comma-formatted, refreshed every 32 frames); Button121 close 1128, Button120 drop-money 1561, EmoneyNavigation 1562, Button4 help 1667; "reif" effect over suspicious items (DlgMpcGoods.cpp:142-153, 189-243). Grid: right-click uses (MyGrid.cpp:584-589 -> MyShellDlg.cpp:7830); left-click picks up to drag to map, hotbar, equipment slot, shop or warehouse (MyShellDlg.cpp:6622-6650, 6968-6980, 7053-7075, 9638-9693); hover tooltip (5451-5457); +N and quality overlays (DlgMpcGoods.cpp:245-292).
- Remake: hand-written (controls.ini:151-185); shows while hero.shop|hero.storage|hero.trading or toggled by 130-1007 / 153-1128. GUI.ini places it right. Opens like the client only for shop, warehouse and trade.
- Mismatches:
  - CPs text 1129 (GUI.ini 118,420, 66x16) missing; no CP binding (DlgMpcGoods.cpp:147-148, 205-207, 211).
  - Money not comma-formatted (StringChange(..,2), DlgMpcGoods.cpp:202-204).
  - 1561 Button120 has no action; the client opens confirm type 5 (amount) then ON_MONEYDROP_ACCEPT drops money (DlgMpcGoods.cpp:307-310; MyShellDlg.cpp:3763-3765; DlgConfirm.cpp:598-602).
  - 1562 EmoneyNavigation has no action; the client opens the [Emoney] URL from ini/oem.ini or ini/Common.ini (MyShellDlg.cpp:3766-3777).
  - 1667 Button4 has no action; the client shows help topic 0x0D in CDlgHelp 215 (DlgMpcGoods.cpp:302-305).
  - Close 1128 / Button42 look dead while a shop, warehouse or trade is open (`when` keeps it drawn) and `toggle` then leaves 153 stuck open after the shop closes; the client hides it (MyShellDlg.cpp:9477-9482; DlgMain.cpp:1214-1217).
  - Not opened with compose 275, oven 278, enchant 105, improve/embed/booth, item lock 329, auction (DlgMain.cpp:1687-1725, 2408-2496, 2728-2765) - workbench grids cannot be fed without it.
  - Shop close does not also hide status 131 or clear repair mode (MyShellDlg.cpp:3784-3797).
  - Grid gestures differ: client left-click picks up, right-click uses (MyGrid.cpp:584-589; MyShellDlg.cpp:6622-6650); remake left-click uses/sells/stores at once; no drag to ground (with confirms, MyShellDlg.cpp:9638-9693), hotbar (6968-6980) or equipment slot.
  - No hover tooltip (MyShellDlg.cpp:5451-5457).
  - No +N / quality overlay and no suspicious "reif" effect (DlgMpcGoods.cpp:155-156, 218-243, 286-292).
  - Repair mode missing (m_bIconFix click opens the repair prompt, MyShellDlg.cpp:6622-6634).
  - No slide-in from the right (DlgMpcGoods.cpp:76-97, 172-187).
  - Bag/287 mutual hiding missing (DlgMpcGoods.cpp:74; MyShellDlg.cpp:8594-8599, 8924-8929).

### 131 CDlgQuery (status) - partial, 11 mismatches
- Client: created DlgMain.cpp:388-389 hidden. Opened by OnMainQuery (Button40, DlgMain.cpp:1149-1181) only when the right slot is empty or holds friend/enemy/mentor/business/role/ALL, after CloseCurDlg(2); toggles if it owns the slot. Hotkeys 20/21/22/43 open it on a page (MyShellDlg.cpp:10207-10251). Hidden by shop close (3790), setup (DlgMain.cpp:1228), hotkey/other setup pages (MyShellDlg.cpp:4160-4171); Esc closes it (DlgQuery.cpp:640-645). Slides in from the left to x=0 (DlgQuery.cpp:396-410, 615-633). Draws Dialog6; Button5 1027 and Button3 1029 close; 2-frame tab checks Check61 1504 role, Check63 1505 weapon (1 px lower), Check64 1506 equipment, Check65 1028 skills; Button4 help 1667; invisible power button 1464 (118x60); the current page and battle-power digits (DlgQuery.cpp:412-424, 438-450, 538-575). Pages created in 5065: 132, 134, 135, 163, 286 (DlgQuery.cpp:455-478) and 287 inside 286 (DlgSkill_HotKey.cpp:583). 276 (title), 279, 300, 301 are not created in 5065 (DlgQuery.cpp:757; DlgEquip.h:39).
- Remake: hand-written (controls.ini:870-906); opens from 130-1503 toggle:131; GUI.ini places it (0,65) and pages at [131-13x] 40,55, matching the client.
- Mismatches:
  - Tab 1028 Check65 (GUI.ini 0,288) and its page 163 missing (DlgQuery.cpp:445, 470-473, 736-747).
  - Tab checks have no value, so the current tab never shows frame 1 (DlgQuery.cpp:483, 653-668, 681-760); fixable now with action=page:132;view:status:role and value=view.status.role.
  - Power button 1464 missing; it toggles 286 at (status x+width, y+30) (DlgQuery.cpp:447, 777-796).
  - Battle-power digits missing (CImageStringEx at x+37-len*7, y+30, scale 0.3/0.25) (DlgQuery.cpp:557-575); no battle-power binding.
  - 1667 help has no action; topic depends on the page (7, 11, 9, 10) (DlgQuery.cpp:762-775).
  - Opening rule differs: only when the right slot is free, closing shop/warehouse via CloseCurDlg(2) (DlgMain.cpp:1149-1181); the remake toggles over anything with the rect-overlap heuristic.
  - Not hidden on shop close, setup opening or the hotkey/other pages (MyShellDlg.cpp:3790, 4160-4171; DlgMain.cpp:1228).
  - Esc does not close it (DlgQuery.cpp:640-645).
  - Check63 not drawn at y+1 (DlgQuery.cpp:420).
  - No slide-in and no open/close sounds (DlgQuery.cpp:396-410, 615-633).
  - Hotkeys 20/21/22/43 missing (MyShellDlg.cpp:10207-10251).

### 132 CDlgQuery_Role (role page) - partial, 5 mismatches
- Client: default page (Check61 set), refreshed on show (DlgQuery_Role.cpp:161-164). Always Dialog7 and texts 1130-1136; base view: 1137, 1566-1568, 1569, four Button70 (1565, 1562-1564); detail view: Dialog31 at +8,+194 with 1152-1155; Button71 1150 and Button72 1151 always (DlgQuery_Role.cpp:108-157).
- Remake: hand-written (controls.ini:907-1046); controls, sections and Dialog31 offset match.
- Mismatches:
  - 1133 PK lacks the PK title prefix (GetPkTitle, "%s%d") and colour (red 20-99, yellow below 0, black from 100) (DlgQuery_Role.cpp:231-244); no conditional colour in the engine.
  - 1134 spouse blank instead of StrRes 10075 when none (DlgQuery_Role.cpp:246-250).
  - 1135 guild blank instead of StrRes 10075 when none (DlgQuery_Role.cpp:252-256).
  - 1152-1155 come from the rules' hero.shown.*; the client computes them from GetMinAtk/GetMaxAtk/GetDef plus item additions, magic attack/defence and gem percentages (and adds nAttackMax to the minimum) (DlgQuery_Role.cpp:270-312).
  - Add buttons do not play Sound/AddPoint.wav (DlgQuery_Role.cpp:331, 348, 365, 382); `sound=` could.

### 134 CDlgQueryWeapon (weapon skills page) - partial, 6 mismatches
- Client: Dialog9; Button90/91/92 class buttons 1519-1521; Button173 1063 / Button174 1064; six Weapon<type> icons 1522-1527; six level images 1528-1530, 1060-1062 drawn as pips; the selected skill's details (DlgQuery.cpp:88-178). Classes: single 4xx, double 5xx, other = everything else (HeroWeaponSkill.cpp:121-131).
- Remake: hand-written (controls.ini:1048-1330); class switch via view:weapon_class; icons ani=Weapon + type.
- Mismatches:
  - Levels drawn as numbers; the client draws Weaponlevel5 pips (26 px apart), then Weaponlevel1 pips (14 px, wrap after 2), or Weaponlevel20 at +38,+4 (MyImage.cpp:537-594; DlgQuery.cpp:174-175).
  - Clicking an icon does nothing; the client shows name+level (StrRes 10115/10018), exp % (10019) or max (10020) and description at (30,5)/(30,23)/(30,41) (DlgQuery.cpp:64-75, 146-166, 252-262).
  - "Other" covers only types 0-399; the client includes every non-4xx/5xx type (600+ missing) (HeroWeaponSkill.cpp:121-131).
  - Paging fixed to 0 / 6; the client steps by 6 while more exist (DlgQuery.cpp:300-315); a third page is unreachable.
  - Skills sorted by type and level-0 ones dropped (gui_data.rs attr_entry); the client keeps arrival order and shows level 0 (HeroWeaponSkill.cpp:121-140; DlgQuery.cpp:224-236).
  - No help topic 11 (131's help has no action) (DlgQuery.cpp:762-775).

### 135 CDlgEquip (equipment page) - partial, 7 mismatches
- Client: refreshed on show (DlgEquip.cpp:168-172). Dialog16; head 1672, neck 1674, ring 1673, armor 1678; left weapon 1675 only for professions 11-15, 21-25, 40-45 over an Image160 mark at +56,+248; right weapon 1677, shoes 1676, treasure 1679, mount 1680; donation 1566 and nobility 1619 images; hero model at +80,+225 (DlgEquip.cpp:174-208, 271-354). Double-click unequips; a carried bag item clicked on is equipped after requirement checks (DlgEquip.cpp:356-690).
- Remake: hand-written (controls.ini:1332-1378), seven 1x1 grids hero.worn.1/2/3/4/5/6/8, click unequips.
- Mismatches:
  - Treasure 1679 (slot 7, GUI.ini 7,217) and mount 1680 (slot 9, 7,164) missing (DlgEquip.cpp:137-140, 199-200).
  - Donation 1566 and nobility 1619 images (nob_<rank>64 / nob_<rank>Font64 by rank and sex) missing (DlgEquip.cpp:290-354).
  - Hero model missing (ShowDummy at +80,+225, DlgEquip.cpp:204-207); `model` has no in-game hero binding.
  - Left-weapon slot 1675 always shown; the client gates it by profession and draws Image160 under it (DlgEquip.cpp:189-197, 274-288).
  - Single click unequips (client needs double-click); equipping by dropping a carried item with requirement checks missing (DlgEquip.cpp:397-690).
  - No hover tooltip (MyShellDlg.cpp:5468-5476).
  - No +N / quality overlay or colour variant (type + colour*1000000) (DlgEquip.cpp:233-250).

### 163 CDlgSkill (skills page of 131) - missing, 1 mismatch
- Client: created DlgQuery.cpp:470-473, selected by Check65 1028. Dialog9; Button200/201/202 class buttons 1172-1174 (moves/magic/XP); Button173 1058 / Button174 1059 paging by 6; six icons 1160-1165 MagicSkillType<id> / XpSkillType<id>; six level pips 1166-1171; selected skill text (DlgSkill.cpp OnInitDialog/Show/SetSlot/ShowSelectedInfo); GUI.ini [163-*].
- Remake: no [131-163] and no [163-*].
- Mismatches:
  - Whole page missing (DlgQuery.cpp:470-473); skill.N could feed the icons; class split, levels and exp text need bindings.

### 286 CDlgBattlePower - draft only, 5 mismatches
- Client: m_DlgQuery.m_DlgBattlePower created DlgQuery.cpp:475-478 (a second instance in 258, DlgNpcEquip.cpp:139-184); toggled by 131-1464 (DlgQuery.cpp:777-796) or 258-1464 (DlgNpcEquip.cpp:392-407). Dialog60; texts 1471-1477, 1485; 1493/1494 (StrRes 10083/10500); five Button601 detail buttons 1466-1470; Button3 close; Button4 help topic 48 (DlgSkill_HotKey.cpp:549-590, 648-676, 685-732).
- Remake: draft (controls.ini:4659-4701); only its close button touches it.
- Mismatches:
  - Never opens (131-1464 / 258-1464 missing) (DlgQuery.cpp:777-796).
  - Texts unbound; no battle-power data (DlgSkill_HotKey.cpp:648-676).
  - Five Button601 detail buttons 1466-1470 missing with their enable rule and link to 287 (DlgSkill_HotKey.cpp:573-576, 711-713, 746-769).
  - Placement: the client moves it beside the status at (262,95) or left of 258 (DlgQuery.cpp:785-789; DlgNpcEquip.cpp:397-400); the remake uses GUI.ini 262,30.
  - Help 1667 (topic 48) has no action.

### 287 CDlgBattlePowerDetail - draft only, 4 mismatches
- Client: created inside 286 (DlgSkill_HotKey.cpp:583), shown by SetMode(1..5); posts 0x449 hiding 258 and the bag (MyShellDlg.cpp:8594-8599); renders after 300 ms. Dialog61; seven rows of item icon 1508-1514, name 1536-1542, detail 1493-1499, score 1500-1506; total 1485; Button3, Button4 (DlgSkill_HotKey.cpp:83-125, 183-216).
- Remake: draft (controls.ini:4642-4657), 3 controls.
- Mismatches:
  - Never opens (DlgSkill_HotKey.cpp:746-769).
  - All seven rows missing (DlgSkill_HotKey.cpp:183-216).
  - 1485 unbound.
  - Opening does not hide the bag or 258; no help topic (MyShellDlg.cpp:8594-8599).

### 152 CDlgNpcBuy (NPC shop) - partial, 14 mismatches
- Client: created DlgMain.cpp:480-481; opened by OpenNpcShop (DlgMain.cpp:1652-1668) when a shop NPC is clicked (read locally from Shop.dat, MyShellDlg.cpp:2054-2063, 2394-2399, 8483-8486); refused while trading or an NPC booth is open; the CP Mall opens it as shop 2888 (MyShellDlg.cpp:8248-8260); shows the bag 153 and sets right owner DLG_NPCBUY; closed by its close button or Esc (DlgNpcBuy.cpp:130-134, 187-191), then ON_NPCBUY_CLOSE hides the bag (MyShellDlg.cpp:3785-3797). Draws Dialog11 (silver) or Dialog58 (CP shop) at (x, y-1), the 4x5 grid, Button121/444/203/204; Button110 (Fix) only in silver shops (DlgNpcBuy.cpp:171-184); slides in from the left over 300 ms with DlgOpen/DlgClose sounds (88-113, 150-169).
- Remake: hand-written (controls.ini:1382-1412), when=hero.shop. Opens like the client for silver shops (the bag follows through 153's `when`).
- Mismatches:
  - CP shops must draw Dialog58 (DlgNpcBuy.cpp:171); the remake always draws Dialog11 and has no shop-currency binding.
  - 1105 Button110 (Fix) has no action; the client posts ON_NPCBUY_FIX (repair cursor; next bag click opens the repair prompt) (DlgNpcBuy.cpp:232-236; MyShellDlg.cpp:4140-4150, 6623-6634).
  - 1105 is always drawn; the client hides it in CP shops (DlgNpcBuy.cpp:183-184).
  - 1667 Button444 (Help) has no action; the client posts ON_HELPDLG_SHOW_5065 (owner DLG_NPCBUY, 45 for CP) (DlgNpcBuy.cpp:262-266).
  - 1550/1551 Button203/204 (page up/down) have no action; the client pages 20 wares with ChangePage.wav (DlgNpcBuy.cpp:238-260); wares past the 20th are unreachable.
  - Buying: the client buys on right-click of 1552 (MyGrid.cpp:589-594 -> MyShellDlg.cpp:7902-7908) or by dragging onto the bag (MyShellDlg.cpp:7059-7066); the remake buys on left-click.
  - A CP-shop buy opens the 290 amount dialog (MyShellDlg.cpp:7905-7907, 8229-8230); the remake buys one directly.
  - Selling is a drag from the bag onto the shop grid with an IMPTITEM_CONFIRM for valuables (MyShellDlg.cpp:7219-7235); the remake sells on a plain bag click with no confirm.
  - Grid cells lack the +N overlay and item colour (DlgNpcBuy.cpp:222-224).
  - No price tooltip rows (MyShellDlg.cpp:6385-6400).
  - No slide-in and no DlgOpen/DlgClose sounds (DlgNpcBuy.cpp:88-113, 150-169).
  - Esc does not close it (DlgNpcBuy.cpp:130-134).
  - The frame is drawn at y-1 in the client (DlgNpcBuy.cpp:177).
  - Not refused while trading or with an NPC booth open (DlgMain.cpp:1654; MyShellDlg.cpp:2060-2062).

### 161 CDlgDepot (warehouse) - partial, 14 mismatches
- Client: created DlgMain.cpp:484/494; opened by server action CMD_ACTION_OPENDEPOT (MyShellDlg.cpp:8291-8293) through OpenDepot (DlgMain.cpp:1670-1682): refused while trading, shows the bag, right owner DLG_DEPOT; closed by 1622/1624 or Esc (DlgDepot.cpp:116-120, 264-270, 455-458). Draws Dialog18, Button180/181/5/3/4, Button203/204 only when enabled (more than one page), edit 1620, money 1623, page statics 1626/1627, the 4x5 grid, the suspicious-item "reif" effect (DlgDepot.cpp:212-232); same slide-in as the shop.
- Remake: hand-written (controls.ini:1485-1536), when=hero.storage. Opens like the client (bag via 153's `when`).
- Mismatches:
  - 1626 (current page static, GUI.ini 10,168) missing (DlgDepot.cpp:229, 342-343).
  - 1627 (page count static, GUI.ini 10,324) missing (DlgDepot.cpp:230, 339-341).
  - 1387/1388 Button203/204 always drawn; the client draws them only with more than one page (DlgDepot.cpp:223-226, 327-336).
  - 1387/1388 have no action; the client pages via SetItem, at most 2 pages (DlgDepot.cpp:460-478); items past the 20th are hidden.
  - 1667 Button4 (Help) has no action (DlgDepot.cpp:480-483).
  - 1623 money should be comma-formatted and right-aligned (SetStaticPosType(2)) (DlgDepot.cpp:167, 281-302); the remake shows plain left-aligned {storage.money}.
  - 1620 lacks the 12-char limit, offset (4,3), white-on-black, digits-only with live commas, focus on open (DlgDepot.cpp:96, 162-164, 404-437).
  - Deposit only goes through when 0 < amount <= hero money (DlgDepot.cpp:442); storage_deposit sends any amount.
  - Grid cells lack quality effect, +N, colour and the suspicious "reif" effect (DlgDepot.cpp:232-262, 352-357).
  - Store/withdraw are drag-and-drop with "bag full" (StrRes 10106) / "warehouse full" (10107) messages and a locked-item confirm (MyShellDlg.cpp:7090-7108, 7332-7357); the remake is a single click with no checks.
  - No item tooltips (DlgDepot.cpp:361-381).
  - No slide-in and no sounds (DlgDepot.cpp:68-98, 191-210).
  - Esc does not close it (DlgDepot.cpp:116-120).
  - Not blocked while trading (DlgMain.cpp:1672).

### 141 CDlgGroup (team) - partial, 13 mismatches
- Client: created DlgMain.cpp:590, hidden (595), at GUI [0-141] (380,0), never moved; main button 1011 toggles it (OnMainGroup, DlgMain.cpp:1258-1270); always rendered last (DlgMain.cpp:1138). Show (DlgGroup.cpp:187-223): Dialog19; five FACE_IMAGE members (ani/PlayerFace.ani, each member's own face) with a focus frame on the selected one; Btn1 1640 / Btn2 1641; Setup 1648 (Button197) and Kick 1649 (Button195) leader only; Close 1642 (Button5), Help 1667 (Button444); auto check 2019 (ChkAutoGroup); state picture 2021 GroupinvitePic in a team else GroupapplyPic (273-280). Btn1/Btn2 by role (250-267, 287-320): no team Button190 Create / Button191 Join cursor; leader Button192 Invite cursor / Button194 Dismiss; member Button193 Leave / Button196 Follow. Member HP is not in this dialog (overhead bars, Player.cpp:586-594; radar dots). Invite/apply requests use CDlgConfirm 158 modes 2/1 (MyShellDlg.cpp:9026-9038; DlgConfirm.cpp:1364-1372).
- Remake: hand-written (controls.ini:1602-1742), toggled by 130-1011. Opens like the client.
- Mismatches:
  - 1643-1647 draw {team.member.N} names; the client draws no names (ShowMemberName empty, DlgGroup.cpp:486-488; Show draws only images 198-205).
  - 9200-9204 always draw PlayerFace1 (team.face.N returns "1"); the client draws each member's own face (DlgGroup.cpp:245-247; MyImage.cpp:180-185).
  - Faces have no click selection or focus frame (OnGroupImg1-5 -> SelectMember, DlgGroup.cpp:204-205, 340-353).
  - Kick 1649 uses pick:team_kick (next world click); the client kicks the selected face and keeps Kick disabled (frame 2 GroupBtnKickoutFalse) until a non-leader face is selected (DlgGroup.cpp:234, 329-338, 352); the engine shows frame 2 on hover.
  - 1648 and 1649 always drawn; leader only in the client (DlgGroup.cpp:209-213) - needs when=team.leads.
  - 1648 toggle:268 works for anyone; leader only in the client (DlgGroup.cpp:322-327).
  - 9223 Button196 (member's second button) runs team_leave; in the client it is Follow (TeamFollow, DlgGroup.cpp:318-319); no follow input.
  - 1667 uses Button4; the client uses Button444, with ON_HELPDLG_SHOW_5065 DLG_GROUP (DlgGroup.cpp:381-384); no action.
  - Closing 141 does not close 268 (DlgGroup.cpp:72-77).
  - Auto check 2019 only flips a local flag; the client disables it for aura-team members, sends CMsgAuraGroup + TeamOpen for an aura leader, else sets auto-apply (DlgGroup.cpp:219-220, 360-377).
  - Double-clicking a face to start a team whisper missing (MyShellDlg.cpp:10805-10822).
  - Invite/apply confirms (158 modes 2/1) missing; the engine prints "/team join X" (game/src/teams.rs:279) and only auto-joins with the auto flag (MyShellDlg.cpp:9026-9038, 4062-4073).
  - No living-player check for Join/Invite picks (MyShellDlg.cpp:1794-1815).

### 268 CMenuGroup (team options) - partial, 3 mismatches
- Client: created by CDlgGroup (DlgGroup.cpp:155) at GUI [0-268] (499,114); draws Check190 Stop, Check191 Money, Check192 Item without background (MenuGroup.cpp:86-88, 98-106); clicks go to TeamClose/TeamOpen, SetTeamMoneyAccess, SetTeamItemAccess (DlgGroup.cpp:109-126).
- Remake: hand-written (controls.ini:1744-1765); sections, bindings, actions right.
- Mismatches:
  - Anyone can open it; leader only in the client (DlgGroup.cpp:324).
  - Not hidden when 141 hides, the team changes, or the bag opens (DlgGroup.cpp:75-76, 229-230; DlgMain.cpp:1210-1211).
  - First show after creating a team does not force Money on / Item off and send both (MenuGroup.cpp:42-49).

### 158 CDlgConfirm (generic yes/no prompt) - draft only, 14 mismatches
- Client: created MyShellDlg.cpp:5093-5095, drawn 1230, 1301, 1574; opened by SetConfirmDlg5065 with a mode (DlgConfirm.cpp:1338-1689). Show5065 (798-862): Dialog15 at (-5,-5) (Dialog45 at (-25,-25) only in mode 14); OK Button150; Cancel Button151 when enabled; Exercise button in mode 27; yellow text; Dialog56/57 icons and edits in modes 5, 9, 10, 11; detail rows in mode 32; modes 0-2 auto-hide after 10 s. Modes used: 0 trade request (MyShellDlg.cpp:8830), 1 team join request (9037), 2 team invite (9030), 3 guild join request (9189), 4 guild invite (9182), 5 drop money (3764), 6 delete friend (2736/4012), 7 delete talk target (4076), 8 leave guild (2662/4297), 9 guild donation (2652/4265), 10/11 booth price (4460/4471), 12 guild kick (2648/4256), 13 open booth (2087/2459/8166), 14 marriage, 15 important item (7231/7629), 16 drop item, 17 gem take-out (2862/4559), 18/19 dice (2152-2467/8163), 20 delete enemy (2865/4594), 21 resource lost (9453), 23 friend request (9463), 24 shop purchase, 25/26 exp ball (4602/7880), 27 exit to offline training (4928), 28 offline notice (8428), 29 blacklist (4649), 30 training map denied, 31 autorun end, 32/39 booth CP buy/share, 33/35 leave mentor, 34 mentor notice (6691/7734), 36/37 business (3082/3986), 38 booth price warning (3146), 40 suspicious trade, 41 dismiss student, 42-44 item lock (3167), 45-51 locked-item notices, 52-54 notices, 55 quest abandon.
- Remake: draft; never opens. Trade requests become a chat line (lib.rs:590-594); team invites only auto-accepted with the auto check (lib.rs:565-577); no yes/no prompt anywhere.
- Mismatches:
  - Never shown; nothing opens it (DlgConfirm.cpp:1338-1689).
  - Frame is Dialog45 at the origin; the client uses Dialog15 at (-5,-5), Dialog45 only in mode 14 (DlgConfirm.cpp:802-808).
  - 1458 text empty; the client shows multi-line yellow text per mode (DlgConfirm.cpp:122-124, 1362-1679).
  - 1456 is type=text ani=Dialog56 (text ignores ani); the client draws a static image only in modes 5, 9, 10, 11 (DlgConfirm.cpp:117-118, 838-845).
  - 1457 same with Dialog57, modes 10/11 only (DlgConfirm.cpp:119-120, 841-845).
  - 1666 edit unbound and always drawn; the client shows an 11-char amount edit on Edit150 only in modes 5, 9, 10, 11 (DlgConfirm.cpp:103-104, 838-845, 1401-1404).
  - 1667 edit unbound and always drawn; CP price edit only in modes 10/11 (DlgConfirm.cpp:105-106, 1417-1421).
  - 1665 OK has no action; the client runs the mode's action then hides (DlgConfirm.cpp:579-748).
  - 1664 Cancel has no action and is always drawn; hidden in modes 30, 34, 35, 52-54; reject actions for team/exit/mentor/trade (DlgConfirm.cpp:751-784, 831-832, 1471-1490).
  - 1463 Exercise always drawn with no action; mode 27 only (DlgConfirm.cpp:786-796, 833-834, 1495-1499).
  - 2003-2006 always drawn and empty; mode 32 only with StrRes 11130/11132 labels (DlgConfirm.cpp:126-135, 846-855).
  - No 10 s auto-hide for modes 0-2 (DlgConfirm.cpp:858-860).
  - Enter/Esc not mapped to OK/Cancel (DlgConfirm.cpp:160-170).
  - No refusal of a second trade request while mode 0 shows (DlgConfirm.cpp:1343-1348).

### 137 CDlgTrade (trade) - partial, 21 mismatches
- Client: created DlgMain.cpp:448-454 and moved to (10, 550-h) = (10, 242), overriding GUI.ini (0, 65). Start: main button 1010 (OnMainChat) refuses while dead (StrRes 10712), else posts ON_MAIN_TRADE (DlgMain.cpp:1246-1255), which prints StrRes 10065 and arms CMD_TRADE (MyShellDlg.cpp:3799-3802); the next player clicked gets ApplyTrade (MyShellDlg.cpp:2316-2318). A request (CMD_TRADEPLAYERAPPLY) opens CDlgConfirm 158 type TRADE_CONFIRM "<name> / StrRes 10000 / 10001" (MyShellDlg.cpp:8824-8832; DlgConfirm.cpp:1363-1378); OK posts ON_TRADE_ACCEPT -> AcceptTrade (DlgConfirm.cpp:222-223; MyShellDlg.cpp:3803-3806), Cancel sends Reject(REJECT_TRADE) (DlgConfirm.cpp:516-517). CMD_OPENTRADE shows trade + bag, hides chest/business/offline result (MyShellDlg.cpp:8834-8850); CMD_TRADEFALSE (StrRes 10109) / CMD_TRADEOK hide it (8866-8876). Draws Dialog14 and the controls below (DlgTrade.cpp:283-377). (Source note: CMD_OPENTRADE sets m_uCurRDlgID = DLG_TRADE at MyShellDlg.cpp:8836 but ShowChildDlg draws the trade only in the left switch, DlgMain.cpp:884-886.)
- Remake: hand-written (controls.ini:1541-1600), when=hero.trading; start via 130-1010 pick:trade; a request only prints "X asks to trade with you." (engine lib.rs:591-594) and accepting means asking back. Opens like the client: partly (no confirm, wrong position).
- Mismatches:
  - Position (10, 242) set in code (DlgMain.cpp:450-453); the remake uses GUI.ini (0, 65).
  - 1149 partner level (yellow, StrRes 10442 + level, GUI.ini 156,28) missing (DlgTrade.cpp:176-183, 244-245, 321).
  - 1150 partner CPs (GUI.ini 215,100) missing (DlgTrade.cpp:248-249, 535-537); no CP binding.
  - 1573 own CP edit (GUI.ini 211,210) missing; OK also commits CPs (DlgTrade.cpp:237-240, 667-671, 735-736).
  - 1142 money should be comma-formatted via StringChange(..,2) (DlgTrade.cpp:532-534).
  - 1570 lacks offset (1,0), 11-char limit, white-on-black, live commas (DlgTrade.cpp:233-236, 580-665).
  - 1145 Button9 should refuse 0 or more money/CPs than owned (StrRes 10045), then latch on frame 1 and disable both edits (DlgTrade.cpp:721-738).
  - 1139 Button8 (Agree) should refuse while typed money is uncommitted (StrRes 10046), then latch on frame 1 and disable own grid, edit and OK (DlgTrade.cpp:740-753); the remake can be clicked repeatedly.
  - 1385/1386 partner page buttons (GUI.ini 152,57 / 152,120) missing; 1386 shows frame 2 when more pages exist (DlgTrade.cpp:271-272, 334-335, 499-500, 772-789).
  - 1387/1572 own page buttons (GUI.ini 152,168 / 152,233) missing (DlgTrade.cpp:273-274, 336-337, 791-808).
  - 1143/1147 partner page / page-count statics missing (DlgTrade.cpp:250-255, 516-520).
  - 1144/1148 own page / page-count statics missing (DlgTrade.cpp:252-257, 478-482); the grids also cannot page past 6 items.
  - 1667 Button4 (Help) has no action (DlgTrade.cpp:766-770).
  - 1619/1566 partner nobility icon (nob_*32) and nobility font image (GUI.ini 261,42 / 228,49) missing (DlgTrade.cpp:275-276, 338-339, 673-694).
  - 2070 add-business-partner button (GUI.ini 226,27; tip StrRes 11148; ApplyBusiness) missing (DlgTrade.cpp:277, 341-350, 717, 810-814); note the client inits "Btn_AddBusinesser" but Control.Ani has [Btn-AddBusinesser] (Control.Ani:2914).
  - 2087 business assessment image businesskaocha1/2/3 (tip StrRes 11147, GUI.ini 230,22) missing (DlgTrade.cpp:278, 351-376, 696-719).
  - 1146 name lacks the dark-blue block behind it and the flashing red blocks over invalid glyphs (DlgTrade.cpp:297, 300-320, 379-451).
  - Grid cells lack quality effect and +N; own grid capped at 20 items with StrRes 10390 (DlgTrade.cpp:457-461, 472-476, 510-514).
  - Offering is a drag onto 1141 with buddy/protection tips, suspicious-item block and locked-item confirm (MyShellDlg.cpp:7246-7297); the remake offers on a plain bag click.
  - An incoming request has no Yes/No confirm and no Reject (MyShellDlg.cpp:8824-8832; DlgConfirm.cpp:222-223, 516-517); 158 is draft only.
  - No tooltips on either grid (MyShellDlg.cpp:5505-5515); the dead-hero refusal (10712) and the 10065 prompt are replaced by the engine's "Click a player (trade)." (DlgMain.cpp:1249-1252).

### 140 CDlgAct (emote bar) - draft only, 6 mismatches
- Client: created DlgMain.cpp:464-465; toggled by Button44 130-1009 -> OnMainAct (DlgMain.cpp:1284-1291). Dialog22, ten ButtonA<action> buttons and Button5 close (DlgAct.cpp:71-99, 134-182); GUI.ini [0-140] 283,628 458x56; each button calls g_objHero.Emotion(n) (193-246).
- Remake: draft (controls.ini:2206); opened like the client; ids and sections right (1536 A230, 1537 A210, 1538 A170, 1539 A150, 1540 A160, 1541 A270, 1088 A1, 1726 A190, 1090 A200, 1091 A250, 1180 Button5 toggle:140).
- Mismatches:
  - None of the ten buttons acts: Emotion 230 cool, 210 laugh, 170 kneel, 150 sad, 160 angry, 270 faint, 1 dance, 190 wave, 200 bow, 250 sit (DlgAct.cpp:193-246); no emote action or input in the engine.
  - No drag of act buttons onto the quick bar (DLG_ACT ON_ICON_PICKUP/DROP, MyButton.cpp:106-127).
  - No Esc close (DlgAct.cpp:116-119).
  - Opening 140 closes 188/191 via the overlap heuristic.
  - No emote hotkeys (user actions 10-19, MyShellDlg.cpp:10180-10209).
  - Main-bar click should play BtnClick.wav (DlgMain.cpp:1286); the engine plays its open/close sounds.

### 191 CDlgPk (PK mode picker) - draft only, 9 mismatches
- Client: created DlgMain.cpp:466-467; toggled by Button47 130-1013 -> OnMainPk (DlgMain.cpp:1293-1300). Four buttons, no background (DlgPk.cpp:108-118): 1639 Button47 Free, 1636 Button49 Monster-only, 1637 Button48 Team-exclude, 1638 Button412 Arrestment (GUI.ini x=0/100/50/150, 43x18); each click SetPkMode and hides (120-142).
- Remake: draft (controls.ini:4333) with one control; opened by 130-1013 toggle:191 like the client.
- Mismatches:
  - 191-1639 Button47 (Free) missing (DlgPk.cpp:90).
  - 191-1636 Button49 (Monster-only) missing (DlgPk.cpp:91).
  - 191-1637 Button48 (Team-exclude) missing (DlgPk.cpp:92).
  - 191-1638 Button412 has no action (PKMODE_ARRESTMENT); no button can set a PK mode (DlgPk.cpp:138-142).
  - No hide after choosing (DlgPk.cpp:123, 129, 135, 141).
  - Main-bar 130-1013 is always Button47; the client swaps Button47/49/48/412 on CMD_CHANGEPKMODE (MyShellDlg.cpp:9048-9064).
  - The client forces PKMODE_ARRESTMENT at login (DlgMain.cpp:2176, 2226); the engine has no PK mode.
  - Opening 191 closes 188/140 through the overlap heuristic.
  - No PK-cycle hotkey (user action 40, MyShellDlg.cpp:10225-10240).

### 189 CDlgXp (XP / revive / land / abort-transform strip) - draft only, 6 mismatches
- Client: created DlgMain.cpp:601-602; shown on CMD_XPFULL -> OnXpFull at bottom right (MyShellDlg.cpp:9198-9203; DlgMain.cpp:2349-2375), hidden on CMD_XPEMPTY (9204-9216); death shows XpSkillType0/5 revive icons (HeroDie, DlgMain.cpp:2383-2401); flight XpSkillType4 (674-678), transform abort XpSkillType3 (MyShellDlg.cpp:9360-9368), booth close XpSkillType2 (DlgMain.cpp:680-694). Icons Magic.Ani XpSkillType<type>; Button173/174 paging.
- Remake: draft (controls.ini:4880); never opened, no XP binding.
- Mismatches:
  - Never shown on XP full; no XP binding (MyShellDlg.cpp:9198-9216).
  - Images 1186-1195 have no ani/value/action; should be XpSkillType<type> posting ON_XP_CHOOSE (select, or self-cast for MAGIC_TARGET_NONE) (MyShellDlg.cpp:4213-4235).
  - 1277/1278 paging no action (DlgXp.cpp OnXpBtnUp/OnXpBtnDown).
  - Death revive mode missing (XpSkillType0/5, RequestReborn5065) (DlgXp.cpp SetReborn; DlgMain.cpp:2383-2401).
  - Land / abort-transform / booth-close modes missing (DlgXp.cpp SetFly/SetTransform/ShowBoothCloseAction; DlgMain.cpp:674-694).
  - GUI.ini [0-189] is 0,0 w=900; the client always repositions bottom-right (DlgXp.cpp MoveActionStrip).

### 214 CDlgKongfu (Kongfu skill strip) - draft only, 5 mismatches
- Client: created DlgMain.cpp:603-604; toggled with 188 by OnMainMagic when GetKongfuAmount()>0 at y=768-223 (DlgMain.cpp:1823-1838); MagicSkillType<type> icons (DlgKongfu.cpp:202); click posts ON_MAGIC_CHOOSE and hides (243-297).
- Remake: draft (controls.ini:3367); never opened.
- Mismatches:
  - Not opened by 130-1022 (DlgMain.cpp:1823-1838).
  - Images 1216-1224, 1577 have no ani/value/action (DlgKongfu.cpp:202, 241-297).
  - Paging 1279/1280 no action.
  - Placed by GUI.ini (0,0), not the client's bottom-right MoveWindow (DlgMain.cpp:1833-1834).
  - No Kongfu-only skill list binding.

### 133 CDlgKnightage (guild) - partial, 20 mismatches
- Client: created DlgMain.cpp:365, moved (366-369); slide-in ends at x=0, final (0,58). Main button 1024 toggles it (OnMainFaction, DlgMain.cpp:1606-1628); also opens after a guild join/invite pick (MyShellDlg.cpp:1831, 1845) and ON_FACTION_BACK (4242); right-slot owner DLG_CONTACT (DlgMain.cpp:984-986). Slides in over 300 ms, queries guild info on open, first open shows the announcement page (DlgKnightage.cpp:106-128, 260-281). Always Dialog8, then one page by mode (283-337): Attributes (Dialog28 at +20,+48; 1039 contribution, 1038 leader, 1037 name, 1040 title, 1041 population, 1042 fund); Announcement (edit 1512, multi-line 256); Announcement edit (Image80 at +34,+57, 1668 Button0, 1671 Button7); Allies/Enemies (statics 1507, 1508, 1030, 1031, 1032); Members (Image81 at +20,+48, list 1370 10 a page "name level" with online flag, 1371 Button383 / 1372 Button382, 1669 Button810 leader). Common buttons (339-350): 1052 Button83 Attributes; 1043 Leave/Members (Button81 / Button87 leader / Button89 sub-leaders, hidden without guild); 1046 Apply/Edit (Button82 / Button88 leader / Button89 member); 1044 Button80 Donate; 1510 Button5; 1047 Button84 Announcement; 1048 Button85 Allies; 1049 Button86 Enemies; 1667 Button4; 1511 Button3. Most disabled without a guild (462-468). The knightage sub-dialogs (288-292, 306, 311, 321) are not compiled in 5065 (MyShell.vcxproj:98); only 257 is created (DlgKnightage.cpp:235).
- Remake: hand-written (controls.ini:1767-1890), toggled by 130-1024; opens but draws every page at once.
- Mismatches:
  - 1039 shows {guild.rank}; the client shows the hero's contribution (DlgKnightage.cpp:495-498); no binding.
  - 1040 shows {guild.rank} again as a number; the client shows the rank title (GetSyndicateTitle) (DlgKnightage.cpp:493).
  - No page modes: attribute texts, list 1370, edit 1512, 1668/1671, 1371/1372, 1669 and relation rows draw together (DlgKnightage.cpp:289-337).
  - Page backgrounds Dialog28, Image80, Image81 missing (DlgKnightage.cpp:291-294, 306-309, 328-331).
  - Does not open on the announcement page the first time (DlgKnightage.cpp:115-119).
  - 1052 Button83 runs guild_tab:members; it is the Attributes page; the member list is reached via Leave (leaders) or Apply (members) (DlgKnightage.cpp:580-583, 600-633).
  - 1047 Announcement has no action; no announcement binding ("NULL" -> StrRes 100158); 1512 unbound and single-line (DlgKnightage.cpp:227-229, 518-526, 585-588).
  - 1048/1049 put allies/enemies into list 1370; the client fills statics 1507/1508/1030/1031/1032 (up to 5) which stay empty in the remake (DlgKnightage.cpp:397-414).
  - 1043 runs guild_leave directly; the client needs contribution >= 20000 and a FACTIONOUT confirm for members, opens the member page for leaders; image swap Button87/89/81 and hiding without a guild missing (DlgKnightage.cpp:340-341, 470-489, 600-617).
  - 1046 has no action (join cursor ON_FACTION_JOIN_5065 / member page / announcement edit); Button88/89 swap and hiding missing (DlgKnightage.cpp:342-343, 619-642; MyShellDlg.cpp:2657-2660).
  - 1044 Donate has no action (FACTIONSUB confirm -> SynContribute) (DlgKnightage.cpp:644-648; MyShellDlg.cpp:2639-2653).
  - 1668 OK / 1671 Cancel have no action (send / clear announcement) (DlgKnightage.cpp:661-676).
  - 1371/1372 paging no action; list lacks level, online state, 10-row pages (DlgKnightage.cpp:435-454, 678-695).
  - 1669 Delete has no action and is always drawn; leader on the member page only, FACTIONDEL confirm (DlgKnightage.cpp:335-336, 697-706; MyShellDlg.cpp:2615-2650).
  - 1667 help no action (ON_HELPDLG_SHOW_5065 8) (DlgKnightage.cpp:575-578).
  - Clicking a member does nothing; the client queries it and opens 257 (DlgKnightage.cpp:159-176; MyShellDlg.cpp:9289-9291).
  - No disabled state without a guild (DlgKnightage.cpp:462-468); 3-frame Button80/83-86 show frame 2 on hover instead.
  - Closing does not hide 257 (DlgKnightage.cpp:568-573).
  - No slide-in, sounds or info query on open; placed at y=65, client final y=58 (DlgMain.cpp:365-369).
  - Guild join/invite confirms (158 modes 3/4) missing (MyShellDlg.cpp:9178-9190).

### 257 CDlgFacState (guild member info) - draft only, 4 mismatches
- Client: created by 133 (DlgKnightage.cpp:235) at (240,65); opened by SetFacState after a member query (MyShellDlg.cpp:9289-9291). Dialog41, name 1201, level 1202, rank 1203, contribution 1204, close 1375 Button5.
- Remake: draft; sections right.
- Mismatches:
  - Nothing opens it (no list-click hook) (MyShellDlg.cpp:9289-9291).
  - Texts unbound.
  - Overlaps 133 (x=240 < 250), so the engine's overlap rule would close 133 when it opens.
  - Not hidden with 133 (DlgKnightage.cpp:568-573).

### 160 CDlgFriend (friends) - draft only, 12 mismatches
- Client: created DlgMain.cpp:442, GUI (0,65,215,370); main button 1026 toggles it (DlgMain.cpp:1183-1203); other contact pages open it via ON_CONTACT_PAGE_5065 1 (MyShellDlg.cpp:2738-2760); slides in (DlgFriend.cpp:77-106, 209-230). Draws GeneralBG, title image DialogFriend 2071, list 1680 (yellow, online first), 1679 Button170 Add, 1681 Button5, 1667 Public_HelpSignBtn, 1682 Button175 Enemy, 2058 btn-teacher, 2082 Btn-Business (232-245); row actions: query -> 209, whisper (mode 5005), leave word (117-157).
- Remake: draft, reachable through 130-1026 toggle:160.
- Mismatches:
  - [0-160] has no ani; the client draws GeneralBG (DlgFriend.cpp:232-237).
  - 2071 is type=text ani=DialogFriend, which draws nothing; it is an image (DlgFriend.cpp:177).
  - List 1680 unbound (no friend data), not yellow, not online-first (DlgFriend.cpp:188, 251-285).
  - 1679 Add no action (ON_FRIEND_ADD_5065 pick cursor, MyShellDlg.cpp:2709-2712).
  - 1682 Enemy no action (-> 269) (DlgFriend.cpp:341-344).
  - 2058 no action (-> 312) (DlgFriend.cpp:346-349).
  - 2082 no action (-> 326) (DlgFriend.cpp:351-354).
  - 1667 help no action (DlgFriend.cpp:325-328).
  - Row actions (query -> 209, whisper, leave word) missing (DlgFriend.cpp:117-157).
  - No slide-in, no DlgOpen/DlgClose sounds (DlgFriend.cpp:77-106).
  - Closing does not hide 209 (DlgFriend.cpp:83-84).
  - List arrows [160-1]/[160-2] (GUI.ini) missing.

### 269 CDlgEnemy (enemies) - draft only, 8 mismatches
- Client: created DlgMain.cpp:443; opened by ON_CONTACT_PAGE_5065 0. GeneralBG, title DialogEnemy, list 1425, close 1424, help 1667, 1703 Button460 Friend, 2058 teacher, 2082 business (DlgEnemy.cpp:108-116, 133-176); rows query -> 270.
- Remake: draft; unreachable (160-1682 has no action).
- Mismatches:
  - No GeneralBG frame (DlgEnemy.cpp:133-176).
  - Title 2071 is a text control; draws nothing.
  - List 1425 unbound.
  - 1703 no action (-> 160) (DlgEnemy.cpp:264-276).
  - 2058 no action (-> 312).
  - 2082 no action (-> 326).
  - Help no action; row click -> 270 missing.
  - No slide-in.

### 209 CDlgFrdState (friend info) - draft only, 7 mismatches
- Client: created by 160 (DlgFriend.cpp:191) at (201,54). Dialog25 at y+1; face 1684; texts 1201-1204, 1206-1208; 1209 hotkeysee (view equipment); 1205 Button250 delete (ON_FRDSTATE_DEL_5065 -> confirm); 1683 close; nobility 1566 and business 2087 only when set (DlgFrdState.cpp:134-158, 206-221).
- Remake: draft; sections right.
- Mismatches:
  - Never opened (DlgFriend.cpp:117-157).
  - Overlaps 160 (x=201 < 215); opening would close the friend list.
  - Texts unbound.
  - Face unbound.
  - 1205 / 1209 no action.
  - Nobility / business not conditional.
  - y+1 frame offset missing (DlgFrdState.cpp:134).

### 270 CDlgEnmState (enemy info) - draft only, 6 mismatches
- Client: like 209 with Dialog47; delete 1428 -> ON_ENMSTATE_DEL_5065, check 1429 (DlgEnmState.cpp:99-133, 200).
- Remake: draft.
- Mismatches:
  - Never opened.
  - Overlaps 269 (201 < 203).
  - Texts unbound.
  - Face 1427 unbound.
  - 1428 / 1429 no action (DlgEnmState.cpp:200).
  - Nobility 1623 / business 2087 not conditional.

### 138 CDlgSetup (options) - draft only (unreachable), 17 mismatches
- Client: created DlgMain.cpp:457-458; Button43 130-1008 -> OnMainSetup (DlgMain.cpp:1220-1242) hides emotion, CloseCurDlg(1), hides Query/Friend/Enemy, shows 138; a second click calls Exit() (apply volumes, hide); user hotkey 45 (MyShellDlg.cpp:10255-10258). Dialog10 at y+2, 2 sliders, 3 checks, 13 buttons, 2 Picture100 radios (DlgSetup.cpp:356-387); 1075, 1084-1086 never drawn.
- Remake: draft (controls.ini:4612); never shown because 130-1008 runs action=menu (the engine's egui Options).
- Mismatches:
  - Unreachable: 130-1008 opens the egui menu, not 138 (DlgMain.cpp:1220-1242).
  - Sliders 1073 (music) and 1074 (effect) missing (Slider0, width 80, max 100, live volume) (DlgSetup.cpp:185-195, 239-247); no slider control type.
  - 1531 Return (Button3) no action (apply volumes, hide) (DlgSetup.cpp:389-400).
  - 1535 Button5 toggle:138 does not apply volumes (DlgSetup.cpp:453-456).
  - 1070 Exit (Button103) no action (ON_EXIT) (DlgSetup.cpp:402-405).
  - 1532 Delete role (Button102) no action (confirm, password, DeleteRole) (DlgSetup.cpp:434-451).
  - 1076/1078/1079/1080 (Button100/101) no action (ON_SETUP_MODECHANGE 0-3) (DlgSetup.cpp:407-419, 473-485).
  - 1533/1534 typed text with ani=Picture100; the client uses CMyPicture radio frames 1/0 (DlgSetup.cpp:279-285).
  - 1072 / 1077 shadow (Button104/105) no action (DlgSetup.cpp:458-466).
  - 1075 drafted with action=toggle:138, no ani, no rect; the client never draws it (DlgSetup.cpp:468-471) - spurious close.
  - 1084/1085/1086 drafted as ani-less buttons; never Init'ed, disabled for English (DlgSetup.cpp:223-231); 1086 has a GUI.ini rect so it becomes an invisible hit area.
  - 1081 Hotkey (Button107) no action (show 231) (DlgSetup.cpp:492-495; MyShellDlg.cpp:4158-4167).
  - 1082 Other (Button108) no action (show 255) (DlgSetup.cpp:497-500; MyShellDlg.cpp:4168-4177).
  - 1667 Help no action (help 0x13) (DlgSetup.cpp:487-490).
  - 1566 MusicSwitch check no value/action (DlgSetup.cpp:569-576).
  - 1083 Check100 (always show names) and 1087 Check101 (KO show) no value/action (DlgSetup.cpp:502-522).
  - Dialog10 drawn at y, client y+2 (DlgSetup.cpp:366).

### 231 CDlgSetup_Hotkey (IDD_DIALOG_HOTKEY) - draft only, 8 mismatches
- Client: created DlgMain.cpp:459-460; opened by Setup 1081 (MyShellDlg.cpp:4158-4167). Dialog32, two lists, old-hotkey static, new combo string at (+246,+126) captured by a hidden CHotKeyCtrl; Button321 revert, Button322 change, Button323 delete, Button4 help, Button3/Button5 close (DlgSetup_Hotkey.cpp OnInitDialog/Show). (DlgSkill_HotKey.cpp is really 286/287 battle power.)
- Remake: draft (controls.ini:4719); close buttons right; unreachable.
- Mismatches:
  - Never opened (138 unreachable, 1081 no action) (MyShellDlg.cpp:4158-4167).
  - 1579 category list (StrRes 10300-10305, 11018) and 1580 action list unbound.
  - 1265 current-hotkey text unbound.
  - New-combo text at (+246,+126) missing; no key-combo capture.
  - 1583 Change no action (duplicate check StrRes 10340).
  - 1578 Revert no action.
  - 1582 Delete no action.
  - 1667 help no action (0x19).

### 255 CDlgOther (setup "Other": furniture) - draft only, 4 mismatches
- Client: created DlgMain.cpp:461-462; shown by Setup's "Other" button (DlgSetup.cpp:497-500 -> MyShellDlg.cpp:4168-4177), hiding 131, Friend, Enemy. Dialog40; Button401 1373 (empty handler in 5065); Button402 delete 1694; Button3 1374, Button5 1180 close; Button4 1667 (DlgOther.cpp:100-163).
- Remake: draft (controls.ini:4079-4104); never opens (Setup is the engine's egui menu).
- Mismatches:
  - Never opens; no Setup "Other" path (DlgSetup.cpp:497-500).
  - 1694 Button402 no action (hide + delete-NPC cursor) (DlgOther.cpp:171-178; MyShellDlg.cpp:4501-4507).
  - 1667 help no action (DlgOther.cpp:161-164).
  - Opening does not hide 131, Friend, Enemy (MyShellDlg.cpp:4168-4177).

### 302 CDlgSetup_Graph (low-FPS restart prompt) - draft only, 5 mismatches
- Client: created MyShellDlg.cpp:5110; shown when FPS <= 5 over the first 600 frames in game mode 1 (7911-7926); hidden on logout (9431). NewLoginMesBox2, StrRes 11075 in yellow, NewLoginRestart, NewLoginNot (DlgSetup_Graph.cpp:72-110).
- Remake: draft (controls.ini:4703); never opened.
- Mismatches:
  - Never opened (MyShellDlg.cpp:7911-7926).
  - Dialog ani Dialog15; the client draws NewLoginMesBox2 (Dialog15 only for size) (DlgSetup_Graph.cpp:79-82, 103-108).
  - 1554 text empty (StrRes 11075) (DlgSetup_Graph.cpp:88-90).
  - 1564 NewLoginNot no action (hide).
  - 1563 NewLoginRestart no action (GameModeRecord=0, restart).

### 274 CDlgTalkAct (emotion icon picker) - draft only, 6 mismatches
- Client: created by CDlgTalk at (800-w, H-64-h) (DlgTalk.cpp:290-294) = GUI.ini 574,586; toggled by 1096. Dialog50 at (+3,-5); 1719 Button444, 1720 Button333, 1721 Button500, 1722 Button501; 32 animated ani/EmotionIco.ani ICON<n> icons (300 ms frames) in 8 columns, 20 px rows (DlgTalkAct.cpp:117-153); icon click inserts #NN at the 1542 caret (DlgTalkAct.cpp:187-202; DlgTalk.cpp:214-216, 863-889); paging by 32 (165-185); help (155-158).
- Remake: draft; opened by 145-1096 toggle:274 (matches the client toggle).
- Mismatches:
  - Icon grid missing; the engine has no EmotionIco.ani source (DlgTalkAct.cpp:117-153).
  - Icon click does not insert into 1542 (DlgTalkAct.cpp:187-202).
  - 1721/1722 paging no action (DlgTalkAct.cpp:165-185).
  - 1719 help no action (DlgTalkAct.cpp:155-158).
  - Dialog50 drawn at 0,0 instead of (+3,-5) (DlgTalkAct.cpp:126).
  - #NN emoticons are never rendered (log or edit) (MyCombo.cpp:98).

### 148 CDlgListBox (whisper-name history) - draft only, 3 mismatches
- Client: created by CDlgTalk (DlgTalk.cpp:284-287); toggled by 145-1185 (795-806); hides when the mouse leaves (376-389); Dialog24 at (x-3,y-4) and a yellow list (DlgListBox.cpp:50-62, 110-125); row click copies the name into 1544 (DlgTalk.cpp:219-229).
- Remake: draft; never opens.
- Mismatches:
  - Nothing toggles it (145-1185 has no action) (DlgTalk.cpp:795-806).
  - Frame at the origin instead of (-3,-4) (DlgListBox.cpp:50-62).
  - 1099 has no name-history binding or row action (DlgTalk.cpp:219-229).

### 139 CDlgChat (chat options: filters and colours) - draft only, 11 mismatches
- Client: created hidden (DlgMain.cpp:468-469); 145-1095 toggles it (MyShellDlg.cpp:4081-4100), also hiding emotion, native chat, transient dialogs and mentor; closing the blacklist reopens it (4533-4541). Dialog36, Image36 at (+90,+21); 1335 Button5, 1334 Button3, 1337 Button361, 1336 Button360, 1667 Button4; checks 1342 Check363, 1338 Check360, 1341 Check362, 1340 Check361, 1343 Check364, 1357 Check366, 1344 Check365; colour images 1347/1345/1339/1346/1348/1358; swatches 1349-1356 (Image360-367) only while choosing (DlgChat.cpp:229-290). Checks default on (191-197) and set filters (318-390); colour image opens the list (392-441); swatch sets the colour (443-558; palette 37-44); Record -> CDlgLog 218 (307-310); Sift -> 239 (312-316).
- Remake: draft; nothing opens it.
- Mismatches:
  - No way to open it (145-1095 has no action) (MyShellDlg.cpp:4081-4100).
  - Image36 at (+90,+21) missing (DlgChat.cpp:244-252).
  - Swatches 1349-1356 always shown; only while choosing in the client (DlgChat.cpp:278-288).
  - 1359 (no ani) / 1360 drafted though the client never shows them (DlgChat.cpp:184-197, 262-276).
  - Checks unbound, drawn off; client defaults on (DlgChat.cpp:191-197).
  - Check clicks do nothing (channel filter) (DlgChat.cpp:318-390).
  - Colour images no action (DlgChat.cpp:392-441).
  - Swatches no action; ChatColorRecord not saved (DlgChat.cpp:443-558; DlgMain.cpp:2137).
  - 1336 Record no action; CDlgLog 218 missing (DlgChat.cpp:307-310).
  - 1337 Sift no action (hide 139, open 239) (DlgChat.cpp:312-316).
  - 1667 help no action (DlgChat.cpp:297-300).

### 239 CDlgBlack (blacklist) - draft only, 8 mismatches
- Client: created hidden (DlgMain.cpp:474-475); opened by CMD_BLACKLIST_OPEN_5065 from 139 Sift or the native chat Blacklist button (MyShellDlg.cpp:3295-3313; DlgChatNative5065.cpp:1292-1298); refreshes on show (DlgBlack.cpp:63-68). Dialog36, Image37 frame 0 at (+90,+21) and frame 1 at (+30,+120), 1667 Button4, 1699 Button3, 1697 Button363, 1698 Button362, 1696 Button5, list 1695, edit 1700 (limit 15) (DlgBlack.cpp:140-184); Del (208-216), Add with confirm StrRes 11017 (218-238; MyShellDlg.cpp:4640-4662); closes reopen 139 (201-206; MyShellDlg.cpp:4533-4541).
- Remake: draft; nothing opens it.
- Mismatches:
  - No way to open it (MyShellDlg.cpp:3295-3313).
  - Both Image37 overlays missing (DlgBlack.cpp:155-166).
  - List 1695 unbound; no blacklist data.
  - Edit 1700 field not readable.
  - 1697 Del no action (DlgBlack.cpp:208-216).
  - 1698 Add no action, no confirm (DlgBlack.cpp:218-238).
  - 1696/1699 toggle:239 but do not reopen 139 (DlgBlack.cpp:201-206).
  - 1667 help no action; list arrows [239-1]/[239-2] missing.

### 240 CDlgLevWord (message board) - draft only, 10 mismatches
- Client: created hidden (DlgMain.cpp:476-477); main-bar 1006 (IDC_MAIN_SKILL, Button410) toggles it via OnMainLeaveWord after CloseCurDlg(1) (DlgMain.cpp:1272-1282); opening queries the bulletin list (DlgLevWord.cpp:104-109); GUI.ini 182,231 660x306. Dialog37; title Image37{mode+1} at (+270,+7); buttons 1705 Button333, 1707 Button371, 1708 Button271, tabs 1712-1716 Button375-379, 1706 Button370, 1711 Button374, 1709 Button272, 1710 Button373, 1704 Button5, 1667 Button444; mode 0 list 1717; modes 1/2 dim block, multi-line edit 1718 (256), OK 1668 Button0 (mode 1), Cancel 1671 (Button7 / Button2); selected tab holds frame 2, Trade selected first (DlgLevWord.cpp:155, 194-263, 390-466); row double-click whispers the author (MyShellDlg.cpp:2682-2692).
- Remake: draft; 130-1006 runs action=chat instead (controls.ini:121-124).
- Mismatches:
  - 130-1006 should toggle 240 (DlgMain.cpp:1272-1282).
  - Title Image371/372/373 missing (DlgLevWord.cpp:194-263).
  - Mode handling missing: list, dim block, edit, OK, Cancel all at once.
  - Tabs need a persistent frame 2 (engine frame 2 = hover only) (DlgLevWord.cpp:390-466).
  - 1706 Send, 1707 Watch, 1710 Del, 1711 Refresh, 1708/1709 paging, 1668 OK have no action; no bulletin system.
  - 1718 must be multi-line.
  - List 1717 has no rows; double-click whisper missing (MyShellDlg.cpp:2682-2692).
  - Cancel skin does not swap to Button2 in watch mode.
  - 1667 help no action.
  - List arrows [240-1]/[240-2] missing.

### 5326 CDlgNorMsg (chat-log line handles) - missing, 3 mismatches
- Client: two instances (DlgMain.cpp:410-421), only the lower drawn (827); its skin Button161 is absent from the 5065 Control.Ani. In 5065 the visible control is the shell's vertical Slider0 line slider (MyShellDlg.cpp:1432-1433, 5057-5074), default 6 lines, saved as LineRecord (9113-9127, 9608, 10479).
- Remake: nothing.
- Mismatches:
  - Number of log lines not adjustable (engine fixed at 7) (MyShellDlg.cpp:5057-5074).
  - No Slider0 line slider (MyShellDlg.cpp:1432-1433).
  - Setting not saved (MyShellDlg.cpp:9113-9127).

### 218 CDlgLog (chat history viewer) - missing
- Client: created MyShellDlg.cpp:981, opened via ON_TALK_LOG from 139 Record and the native History button (MyShellDlg.cpp:4109-4130).
- Remake: no [0-218].
- Mismatches: whole dialog missing (MyShellDlg.cpp:4109-4130).

(345 CDlgTalkTarget and 280 CDlgMsg are compiled but never instantiated in 5065; their absence is correct.)

### 2232 / 2233 / 2234 native private chat windows and tray - missing, 5 mismatches
- Client: 2232 created hidden (DlgMain.cpp:470-473), Win32/GDI with bitmaps from data/interface/Style01/chatsystem (DlgChatNative5065.cpp:767-800), geometry from CHATGUI.ini (913-951). A whisper to the hero or a team line from another player is stored per conversation and posts 0x450 (MsgTalk.cpp:1134-1161), adding/flashing a tray icon (MyShellDlg.cpp:3325-3351); double-click opens the session (DlgMain.cpp:2532-2548; ChatTray5065.cpp). Window: history, editor (255 chars), Send, Close, Minimize, Emotion popup 2233 (ani/ChatEmotion.ani), History, Blacklist, Details, Help, Spouse; faces, battle power, level, guild (DlgChatNative5065.cpp:802-818, 1132+). Tray (ChatTray5065.cpp, drawn MyShellDlg.cpp:1425): CSConceal at [0-3] 610,650, up to 6 face/TeamIcon icons flashing with CSSashGloom, CSMore at [0-4] 840,657 toggling list 2234.
- Remake: nothing.
- Mismatches:
  - No per-conversation private/team windows (DlgMain.cpp:2532-2548).
  - No tray icons, flashing or conceal toggle (ChatTray5065.cpp).
  - No session list 2234.
  - No emotion popup 2233.
  - No peer details (faces, level, battle power) (DlgChatNative5065.cpp:802-818).

### 294 CDlgBroadcast5065 (pigeon broadcast board) - draft only, 9 mismatches
- Client: created DlgMain.cpp:398, drawn 837-838; the banner's ad_query toggles it (DlgPigeonBanner5065.cpp:173-176; MyShellDlg.cpp:8316-8318); hidden by CMD_PIGEON_HIDE_5065 and logout (8324-8326, 9436); opens on the Upcoming tab and queries (DlgBroadcast5065.cpp:140-147). ad_MainBG; 10 rows (index, name with ad_ContactBtn, text, clock, ad_urgent5/ad_frist); ExerciseHelp, Button5; tabs ad_queue/ad_myqueue/ad_publish/ad_my with frame 1 when selected (265-273); 1524 Pic_channel swapped by mode; status 1885, separator 2008; page buttons or inner arrows by mode; tooltips (675-768).
- Remake: draft.
- Mismatches:
  - No way to open it (295 not shown, 1515 no action) (MyShellDlg.cpp:8316-8318).
  - [0-294] lacks ad_MainBG.
  - Row controls (1904-1953, 1486-1492, 1561/1562/1565, 2009-2018) missing.
  - Tabs have no action and no selected frame (DlgBroadcast5065.cpp:265-273).
  - 1524 does not swap image by mode.
  - Page and inner arrows always shown and overlap at 302,370.
  - 2008 / 1885 have no text (DlgBroadcast5065.cpp:196-197).
  - 1621 help no action.
  - No pigeon queue/history data, no tooltips.

### 296 CDlgPigeonPrompt5065 (broadcast prompts) - draft only, 5 mismatches
- Client: created MyShellDlg.cpp:5106, drawn 1455-1456; ShowMode 54-59 (DlgPigeonPrompt5065.cpp:117-165) from level-under-50 (MyShellDlg.cpp:3200-3204), confirm (3205-3210), urgent/first (3211-3236), results (8297-8299, 8367-8370). Dialog15, message 1518, detail 1519, level 1537, buttons 1522/1523/1525 only when enabled (167-187); confirm actions by mode (189-204).
- Remake: draft.
- Mismatches:
  - Nothing opens it.
  - Buttons and texts always shown instead of by mode (DlgPigeonPrompt5065.cpp:167-187).
  - 1522 confirm no action (DlgPigeonPrompt5065.cpp:189-204).
  - 1523 cancel no action.
  - Texts unbound.

### 293 CDlgOfflineMessage5065 (whisper to an offline friend) - draft only, 4 mismatches
- Client: created MyShellDlg.cpp:5109, drawn 1452-1453; whispering an offline friend queues the line (Hero.cpp:1849-1858; GameMsg.cpp:1255-1283) and CMD_OFFLINE_PRIVATE_PROMPT_5065 opens it (MyShellDlg.cpp:8232-8245). Dialog15 at (-5,-5), 1533 Button150, 1534 Button151, text 1535 (StrRes 11022); confirm sends as a leave-word, cancel discards (3173-3191).
- Remake: draft.
- Mismatches:
  - Nothing opens it; no offline-friend check (MyShellDlg.cpp:8232-8245).
  - 1533 / 1534 no action (MyShellDlg.cpp:3173-3191).
  - Text unbound.
  - Dialog15 drawn at 0,0 instead of (-5,-5).

### 229 CDlgEmbed (gem sockets) - partial, 13 mismatches
- Client: created DlgMain.cpp:520, drawn 948; OpenEmbed (DlgMain.cpp:2447-2465) also shows the bag, when an EMBED-type NPC is clicked (MyShellDlg.cpp:2106-2107, 2421-2422, 8508-8510). Show (DlgEmbed.cpp:209-261): Dialog29, item slot 1688, gem slots 1689/1685 only for existing sockets on a Dialog30 background, ImageFocus on the selected socket, Button290, Button291, Button333, Button5, Button444.
- Remake: hand-written (controls.ini:1939-1980); button skins match.
- Mismatches:
  - Nothing opens 229: no window/when and only its own buttons toggle it (MyShellDlg.cpp:2106-2107, 8508-8510).
  - Bag 153 not opened with it (DlgMain.cpp:2460-2464).
  - 1688 (work.0) takes any first bag item; the client takes only socketed helmet-to-shoes equipment and routes gems/equipment by type (DlgEmbed.cpp:294-300, 507-536).
  - 1689/1685 always active; enabled only for existing sockets, with Dialog30 behind (DlgEmbed.cpp:242-251, 308-329).
  - Gems already set are not shown (Item700000+gem, occupied) (DlgEmbed.cpp:277-285, 312-327).
  - No socket selection or ImageFocus (DlgEmbed.cpp:252-255, 359).
  - 1686 sends combine:1:1:0,1;combine:1:2:0,2 by position (first gem always to socket 1, even when filled); the client embeds only slots holding a newly placed gem (DlgEmbed.cpp:401-408).
  - Embed does not close the dialog (DlgEmbed.cpp:409).
  - 1687 combine:2:1:0 always empties socket 1 without selection or confirm; the client needs a selected filled socket and confirm 158 mode 17, and can empty socket 2 (DlgEmbed.cpp:412-443; MyShellDlg.cpp:2861-2862, 4558-4559).
  - toggle:229 does not clear the workbench (only after a combine, lib.rs:321-323); leftovers carry into 275 which shares work.N (DlgEmbed.cpp:450-473).
  - One click on a slot removes and shifts later slots (lib.rs retain); the client removes on double-click, without shifting (DlgEmbed.cpp:70-72, 372-399).
  - 1667 help no action (ON_HELPDLG_SHOW_5065 0x18) (DlgEmbed.cpp:445-448).
  - Slot icons lack +N (DlgEmbed.cpp:274).

### 275 CDlgItemCompose5065 (+N composing) - partial, 12 mismatches
- Client: created DlgMain.cpp:488, drawn 1000-1002. MsgAction 126 data 1 posts 0x43E (3drole/Network/MsgAction.cpp:733-734) -> OpenItemCompose (MyShellDlg.cpp:8226-8227; DlgMain.cpp:1716-1725): refused while trading, closes the right owner, shows the bag. ShowRenderer (DlgItemCompose.cpp:248-309): Dialog51, Button444/5/333/555, main slot 1437, treasure slots 1440/1441 with Image510 at (136,50)/(136,96) once a main item is in, gem slots 1438/1439 with Image511 at (44,134)/(92,134) when the item is +5 or more.
- Remake: hand-written (controls.ini:1892-1937), window=1 is right; skins match.
- Mismatches:
  - No trade check, right owner not closed (DlgMain.cpp:1718-1720).
  - Bag 153 not opened with it (DlgMain.cpp:1722-1723).
  - 1437 takes any first bag item; equipment only, no stones, coloured or +9 items (DlgItemCompose.cpp:329-385).
  - 1440/1441 always active, no Image510; disabled until a main item is in (DlgItemCompose.cpp:199-200, 285-296, 374-375).
  - 1438/1439 always active, no Image511; enabled only above +4 (DlgItemCompose.cpp:297-308, 376-380).
  - Bag clicks fill slots strictly in order (a gem clicked second lands in treasure slot 1440); the client checks type/family/plus/quality and routes (DlgItemCompose.cpp:517-592, 693-735).
  - 1668 combine:3 sends unchecked; needs main + both treasures, and gems from +5 (DlgItemCompose.cpp:621-651).
  - Compose does not close the dialog (DlgItemCompose.cpp:653).
  - Close buttons do not clear the workbench (DlgItemCompose.cpp:656-686).
  - Clicking a slot removes and shifts; the client has no remove handler (DlgItemCompose.cpp:101-113).
  - 1667 help no action (DlgItemCompose.cpp:311-314).
  - Slot icons lack quality effect and +N (DlgItemCompose.cpp:46-60).

### 211 CDlgImprove (raise quality / level) - draft only, 6 mismatches
- Client: created DlgMain.cpp:504, drawn 942; OpenImprove (DlgMain.cpp:2408-2429) also shows the bag, on a FORGE-type NPC (MyShellDlg.cpp:2103-2104, 2418-2419, 8504-8506). Dialog26, item and gem slots, Button260, Button5, Button333, Button444 (DlgImprove.cpp:163-201).
- Remake: draft; never opens.
- Mismatches:
  - No opener; bag not opened with it (DlgMain.cpp:2408-2429).
  - 1628 unbound (equipment sort 1-8 at full durability) (DlgImprove.cpp:236-263).
  - 1629 unbound (DragonBall for quality, Meteor/MeteorTear for level) (DlgImprove.cpp:203-234).
  - 1630 Button260 no action (ImproveQuality / UpLevelItem then close; the protocol maps these to Input::UseOn, but no GUI action produces it) (DlgImprove.cpp:265-281).
  - 1667 help no action (DlgImprove.cpp:325-328).
  - No double-click removal (DlgImprove.cpp:304-318).

### 278 CDlgOven (refine / fuse) - draft only, 6 mismatches
- Client: created DlgMain.cpp:486, drawn 994; MsgAction 126 data 2/3 posts 0x440/0x441 (MsgAction.cpp:736-741) -> OpenOven (MyShellDlg.cpp:8285-8290; DlgMain.cpp:1687-1696) which also shows the bag. Dialog54, probability/level texts, Button540, Button541 check, Button4, Button5, Button3, 4x5 grid (DlgOven.cpp:158-199).
- Remake: draft; no window=2/3.
- Mismatches:
  - Never opened; bag not opened with it (DlgMain.cpp:1687-1696).
  - Grid 1443 unbound; up to 20 checked items with quality/+N; the engine workbench holds 5 (DlgOven.cpp:245-288, 337-393; gui_data.rs WORK_SLOTS).
  - Check 1668 no value/action (freeze list, refresh probability) (DlgOven.cpp:290-325, 454-458).
  - 1635/1442 unbound (StrRes 10453/10454 while checked).
  - 1634 no action (requires the check, MsgRefine 1028, close) (DlgOven.cpp:439-452).
  - 1667 help no action (Help43) (DlgOven.cpp:433-437).

### 105 CDlgEnchant - draft only, 4 mismatches
- Client: created DlgMain.cpp:487, drawn 998; opens on 0x443 (MyShellDlg.cpp:8294-8295 -> DlgMain.cpp:1701-1710) but nothing in 5065 posts 0x443 (MsgAction 126 maps only 1, 2, 3, 4, 44, MsgAction.cpp:729-750), so it is unreachable in the client too. Dialog55, target and gem slots, Button542, Button5, Button333, Button444.
- Remake: draft; never opens (matches the client).
- Mismatches:
  - 1445 unbound (equipment sort 1-8, full durability, enchant < 255) (DlgEnchant.cpp:256-289).
  - 1444 unbound (gems only) (DlgEnchant.cpp:291-308).
  - 1446 Button542 no action (MsgItem ENCHANT then close; combine:4 exists) (DlgEnchant.cpp:310-326).
  - 1667 help no action (Help44) (DlgEnchant.cpp:328-331).

### 298 CDlgBrokenRepair5065 - draft only, 5 mismatches
- Client: created MyShellDlg.cpp:5107, drawn 1459; opens in shop repair mode when a broken bag item is clicked, checking for 5 Meteors (MyShellDlg.cpp:6563-6584, 6624-6633). Dialog15, yellow text, Confirm/Cancel or Close only (DlgBrokenRepair5065.cpp:110-154).
- Remake: draft; never opens.
- Mismatches:
  - No opener (MyShellDlg.cpp:6563-6584).
  - All three buttons drawn together; 1551/1552 only when repair is possible, 1553 only when not (DlgBrokenRepair5065.cpp:122-124, 148-153).
  - 1551 no action (ON_BROKEN_REPAIR_CONFIRM_5065) (DlgBrokenRepair5065.cpp:156-162).
  - 1552 no action (hide) (DlgBrokenRepair5065.cpp:164-167).
  - 1554 unbound (StrRes 11053 / 11052, yellow) (DlgBrokenRepair5065.cpp:98-100, 118-134).

### 212 CDlgNpcFace (change avatar) - draft only, 6 mismatches
- Client: created DlgMain.cpp:508, drawn 945; OpenNpcFace (DlgMain.cpp:2431-2445) when a FACE-type NPC is clicked (MyShellDlg.cpp:2100-2101, 2415-2416, 8498-8502). Dialog27, Button5, Button333, Button322, Button271/272, six PlayerFace.ani faces (1.. male, 201.. female) with ImageFocus on the pick (DlgNpcFace.cpp:185-298, 423).
- Remake: draft; never opens.
- Mismatches:
  - No opener (MyShellDlg.cpp:2100-2101).
  - Six images have no ani/value; need face bindings (DlgNpcFace.cpp:269-298, 423-448).
  - Face click does not select, no focus (DlgNpcFace.cpp:244-265, 300-334).
  - 1215/1575 Button271/272 paging no action (DlgNpcFace.cpp:336-399).
  - 1214 Button322 no action (ChangeFace then close) (DlgNpcFace.cpp:400-411).
  - No slide-in (DlgNpcFace.cpp:185-215).

### 233 CDlgMpcBooth (own booth) - draft only, 11 mismatches
- Client: created DlgMain.cpp:537-538; OpenBooth(true) (DlgMain.cpp:2469-2496) when the hero clicks its own booth NPC (MyShellDlg.cpp:2128-2129, 2442-2445): left owner DLG_MPCBOOTH, bag shown. Draws Dialog34, Button333/203/343/204/5/444, six slot images, prices and coin images (DlgMpcBooth.cpp:252-347).
- Remake: generated draft (controls.ini:3404-3541); never opens (no when/window/toggle).
- Mismatches:
  - Never opens; the engine has no booth state or binding (DlgMain.cpp:2469).
  - Change buttons 1557/1559/1118/1120/1122/1124 drafted with Button340, which is not in Control.Ani (only Control1/2.Ani); the client never draws them (DlgMpcBooth.cpp:299-317, 401-404).
  - GetBack buttons 1558/1560/1119/1121/1123/1125 (Button341) should draw only for filled slots and call DelItem (DlgMpcBooth.cpp:306-317, 406-470).
  - 1455-1460 drafted as text; in the client they are Dialog56 (silver) / Dialog57 (CP) image statics shown per filled slot (DlgMpcBooth.cpp:235-240, 334-345, 574-589, 667).
  - 1304-1309 item images (Item<type> with quality, +N) unbound; clicking an empty slot with a picked item adds it with a price confirm (DlgMpcBooth.cpp:472-566, 591-669; MyShellDlg.cpp:4421-4461).
  - 1310-1315 prices unbound (FormatItemPrice).
  - 1288/1289 paging has no action (DlgMpcBooth.cpp:362-389).
  - 1291 Button343 (hawk) has no action; the client posts ON_MPCBOOTH_HAWK_5065 -> opens 235 (DlgMpcBooth.cpp:392-399; MyShellDlg.cpp:3287-3291).
  - 1286/1287 only toggle; the client posts ON_MPCBOOTH_CLOSE_5065, hiding hawk and bag (DlgMpcBooth.cpp:349-359; MyShellDlg.cpp:3894-3910).
  - 1667 (Help) has no action (DlgMpcBooth.cpp:726-729).
  - No slide-in and no sounds (DlgMpcBooth.cpp:133-156, 256-283).

### 234 CDlgNpcBooth (another player's booth) - draft only, 8 mismatches
- Client: created DlgMain.cpp:540-541; OpenBooth(false) after m_objBooth.Open when another player's booth NPC is clicked (MyShellDlg.cpp:2130-2139, 2446-2453); no bag. Draws Dialog34, six Button350 buys, Button333/203/204/5/444, images, prices, coins (DlgNpcBooth.cpp:213-301).
- Remake: generated draft (controls.ini:3769-3878); never opens.
- Mismatches:
  - Never opens; no booth support (DlgMain.cpp:2469).
  - 1597-1602 Button350 (Buy) have no action; the client buys with silver or opens a CP confirm (DlgNpcBooth.cpp:479-522).
  - 1607-1612 item images unbound (DlgNpcBooth.cpp:303-381).
  - 1322, 1317-1321 prices unbound.
  - 1457-1462 drafted as text instead of Dialog56/57 image statics per filled slot (DlgNpcBooth.cpp:193-198, 288-299, 380).
  - 1603/1604 paging has no action (DlgNpcBooth.cpp:450-477).
  - 1605/1606 only toggle; the client posts ON_NPCBOOTH_CLOSE_5065 closing the booth (DlgNpcBooth.cpp:432-442; MyShellDlg.cpp:2694-2700).
  - 1667 (Help) no action (DlgNpcBooth.cpp:444-447); no slide-in.

### 235 CDlgHawk (booth cry-out) - draft only, 3 mismatches
- Client: created DlgMain.cpp:544-545; opened by the booth's hawk button (MyShellDlg.cpp:3287-3291), hidden when the booth closes (3900). Draws Dialog35, Button5, Button270, edit 1613 (DlgHawk.cpp:125-150); GUI.ini 0,399 484x130.
- Remake: draft (controls.ini:2984-2999), sections right; never opens.
- Mismatches:
  - Never opens (MyShellDlg.cpp:3287-3291).
  - 1324 (Agree) has no action; the client filters the text, SetCryOut, hides (DlgHawk.cpp:157-167).
  - 1613 unbound; the client edit has a 64-char limit, white on black, cleared on show (DlgHawk.cpp:73, 109-110).

### 232 CDlgMsgSend (leave a word) - draft only, 6 mismatches
- Client: created DlgMain.cpp:529-530 as a middle owner; opened by the Friend/Business "leave word" button (MyShellDlg.cpp:2722-2733, 2807-2818) and Reply in 238 (3061-3068). Draws Dialog33, Button331/3/5/330/4, name 1284, edit 1594 (DlgMsgSend.cpp:137-167).
- Remake: draft (controls.ini:3584-3615), sections right; never opens.
- Mismatches:
  - Never opens (MyShellDlg.cpp:2722-2733).
  - 1284 name unbound (SetTarget, DlgMsgSend.cpp:197-218).
  - 1594 should be a multi-line edit, 256 chars, offset (2,3) (DlgMsgSend.cpp:120-123); unbound and the engine edit is single-line.
  - 1596 (Send) no action (LeaveWords2Friend, DlgMsgSend.cpp:186-195).
  - 1595 (View All) no action; disabled with no words, else opens 238 (DlgMsgSend.cpp:179-184, 216; MyShellDlg.cpp:3054-3060).
  - 1667 (Help) no action (DlgMsgSend.cpp:169-172).

### 238 CDlgMsgReceive (received words) - draft only, 6 mismatches
- Client: created DlgMain.cpp:533-534; opened by View All in 232 (MyShellDlg.cpp:3054-3060). Draws Dialog39, Button3/5/332/335/334/373/4, name 1331, read-only multi-line 1332 (DlgMsgReceive.cpp:143-174).
- Remake: draft (controls.ini:3543-3582), sections right; never opens.
- Mismatches:
  - Never opens (MyShellDlg.cpp:3054-3060).
  - 1331/1332 unbound (DlgMsgReceive.cpp:196-222).
  - 1615/1614 Previous/Next have no action or enable states (DlgMsgReceive.cpp:224-272).
  - 1333 (Reply) no action; enabled only for an offline friend (DlgMsgReceive.cpp:186-194, 214-219).
  - 1616 (Delete All) no action (DlgMsgReceive.cpp:274-278).
  - 1667 (Help) no action (DlgMsgReceive.cpp:181-184).

### 290 CDlgItemPurchase5065 (+292 CDlgBuyMessage5065) (CP buy amount) - draft only, 10 mismatches
- Client: created DlgMain.cpp:493/501, 292 as its child (DlgItemPurchase5065.cpp:291); OpenByType from CMD_ITEM_PURCHASE_5065 when buying in a CP shop (MyShellDlg.cpp:8229-8230, 7902-7908, 7065); refuses with StrRes 10505 / 10506 (DlgItemPurchase5065.cpp:385-427). Draws VipInfoBG, VipInfo1 footer at (+21,+130) and state-dependent controls (514-563).
- Remake: draft (controls.ini:3151-3214); 292 missing.
- Mismatches:
  - Never opens (MyShellDlg.cpp:8229-8230).
  - [0-290] has no ani; needs VipInfoBG plus the VipInfo1 footer (DlgItemPurchase5065.cpp:524-531).
  - 1524 item icon Item<type> unbound (DlgItemPurchase5065.cpp:421-423).
  - 1525 and 1537 both drafted; only one shows by client version (1537 edit on non-Chinese) (DlgItemPurchase5065.cpp:510-511, 534-540).
  - Three modes (amount / limit message / confirm) drawn all at once; 1528-1532 overlap 1521/1522/1526/1529 (DlgItemPurchase5065.cpp:483-512, 544-562).
  - 1513-1516 VipNO02/05/10/Max have no action (DlgItemPurchase5065.cpp:614-648).
  - 1517 unbound; 15-char limit, offset (4,2), VipInfo2 background, cyan text (DlgItemPurchase5065.cpp:282, 293-295).
  - 1511/1512 no action; amount -> confirm -> MsgItem ITEMACT_BUY with amount (DlgItemPurchase5065.cpp:565-612).
  - Text colours missing (1529-1531 yellow, name in quality colour) (DlgItemPurchase5065.cpp:307-309, 379-381).
  - 292 missing: PublicInfoBG, texts 1520-1522, button 1668 ExeConfirm (DlgItemPurchase5065.cpp:37-169; GUI.ini [290-292]).

### 330 CDlgEmoneyWithdraw5065 (CP withdraw button) - draft only, 2 mismatches
- Client: created DlgMain.cpp:491/500; shown on MsgInteract INTERACT_WITHDRAW_READY (MsgInteract.cpp:403-404 -> MyShellDlg.cpp:8269-8270). GUI.ini 480,620; button 1571 at 22,22 80x50.
- Remake: draft (controls.ini:2761-2766), Btn_WithdrawEMoney right.
- Mismatches:
  - Never shows; no "withdraw ready" binding (MyShellDlg.cpp:8269-8270).
  - 1571 no action; the client sends INTERACT_WITHDRAW_EMONEY and hides, result confirm StrRes 11200 (DlgEmoney5065.cpp:180-186; MyShellDlg.cpp:8271-8279).

### 277 CDlgPortableChest5065 (portable chest) - missing, 6 mismatches
- Client: created DlgMain.cpp:485/495; opens when a chest item is used from the bag (refused in a booth StrRes 10445 or trading 10446), hides team and warehouse, LayoutGrid((type%10)/3), OpenPackage(PACKAGE_CHEST) (MyShellDlg.cpp:7840-7860); hidden when a trade opens (8837-8838). Draws Dialog53 at frame rows-1, Button5 and the grid (DlgPortableChest5065.cpp:142-159).
- Remake: the [0-277] draft (controls.ini:3001-3016) is CDlgHead's (shares IDD 277, not instantiated in 5065).
- Mismatches:
  - Wrong dialog drafted: ani=Dialog52 (not in Control.Ani) and controls 1457/1396/1458/1456 (DlgPortableChest5065.cpp:142-159).
  - Frame must be Dialog53 at frame rows-1 with a row-dependent window region (DlgPortableChest5065.cpp:113-119, 152).
  - Grid 1723 missing: rows x 3 at (16,38), 128 x (rows*43-1), placed in code (DlgPortableChest5065.cpp:120-122).
  - 1724 Button5 (GUI.ini 135,22) missing (DlgPortableChest5065.cpp:89, 102-105).
  - No chest package binding or open path (MyShellDlg.cpp:7840-7860).
  - Storing is a drop with "full" (10107) and "chest in chest" (10441) refusals; no "reif" effect (MyShellDlg.cpp:7152-7172; DlgPortableChest5065.cpp:161-188).

### 329 CDlgItemLock5065 (item lock) - draft only, 7 mismatches
- Client: created hidden DlgMain.cpp:2708-2710; toggled by the 328 button via CMD_ITEM_LOCK_TOGGLE_5065 -> ToggleItemLock5065 (MyShellDlg.cpp:3037-3040; DlgMain.cpp:2728-2765), also showing the bag. Draws EquipTie_TieBG; grid 2191 1x1 (dropped bag item); lock icon 2190 (EquipTie_TieIco / EquipTie_UnTieIco); name 2044; tip 2066 (StrRes 11194/11195/11199); bind button 2188 (EquipTie_TieBtn / RavelTieBtn / ReTieBtn by state); close 1180 EquipTie_CloseBtn (DlgItemLock5065.cpp:391-420, 459-600).
- Remake: draft (controls.ini:3117-3141).
- Mismatches:
  - Never opens; 328-2186 has no action (DlgMain.cpp:2728-2765).
  - Dialog has no ani; should be EquipTie_TieBG (DlgItemLock5065.cpp:576-581).
  - Grid 2191 unbound; client accepts a dropped lockable item and selects it on click (DlgItemLock5065.cpp:476-504, 535-545).
  - 2190 drafted as text; should be the EquipTie_TieIco/UnTieIco image (DlgItemLock5065.cpp:506-533).
  - 2044 / 2066 texts unbound (DlgItemLock5065.cpp:454-468).
  - 2188 has no ani or action; image by lock state, disabled until selected, opens the apply/cancel/reapply confirm (DlgItemLock5065.cpp:506-566).
  - Closing does not clear the right slot (CloseItemLock5065, DlgMain.cpp:2768-2774).

### 258 CDlgNpcEquip (another player's equipment) - draft only, 10 mismatches
- Client: created DlgMain.cpp:254. Main-bar Button411 check 1016 (OnMainNpcEquip, DlgMain.cpp:1397-1403) arms CMD_NPCEQUIP (MyShellDlg.cpp:2701-2706); clicking a player sends MsgAction actionQueryChatterEquipment (2323-2331); the reply CMD_FLASH_OTHERPLAYER_EQUIPMENT calls SetNpcEquip() which shows it (9357-9359; DlgNpcEquip.cpp:306-341). Hidden by CloseCurDlg(2), improve/embed/booth (DlgMain.cpp:1506-1508, 2425, 2463, 2482) and 287. Slides in from the right; GUI.ini 768,100. Draws Dialog42; player model at +120,+295; nine item icons 1376-1382, 1385, 1386; nobility 1625 with hover tip (StrRes 11100-11107); Image160 at +90,+302; Button5 1384, Button3 1383, Button4 1667 (topic 36); name 1389 and spouse 1618; battle-power digits; invisible power button 1464 (DlgNpcEquip.cpp:193-251, 343-380).
- Remake: draft (controls.ini:3880-3936); never opens.
- Mismatches:
  - Never opens: 130-1016 has no action; no view-equipment pick or data (DlgMain.cpp:1397-1403).
  - Item slots are images with no value; the client shows the player's items with quality/+N and tooltips (DlgNpcEquip.cpp:253-271; MyShellDlg.cpp:5459-5466).
  - Nobility image 1625 (nob_*64) and tooltip missing (DlgNpcEquip.cpp:273-304, 343-366).
  - 1389 name / 1618 spouse unbound (spouse blanked when StrRes 100122) (DlgNpcEquip.cpp:308-313).
  - Player model and Image160 mark missing (DlgNpcEquip.cpp:223-238).
  - Battle-power digits missing (DlgNpcEquip.cpp:368-380).
  - 1464 has no action (toggle 258's own 286) (DlgNpcEquip.cpp:392-407).
  - 1667 help (topic 36) has no action.
  - Not closed when other right-side dialogs open (DlgMain.cpp:1506-1508).
  - No slide-in.

### 241 CDlgEffigy (statue placement) - draft only, 9 mismatches
- Client: created DlgMain.cpp:515; opened on MsgNpc EVENT_LAYNPC for a statue NPC (MsgNpc.cpp:66-71 -> CMD_OPEN_PLACE_STATUARY_DIALOG, MyShellDlg.cpp:9266) through OpenEffigy (DlgMain.cpp:2497-2502). Slides in from the left; GUI.ini 0,65. Dialog38; Button383/381/0/382/5/333/444; ButtonA230/A210/A170/A150/A160/A1/A190/A200; hero model at +100,+275 (DlgEffigy.cpp:146-251).
- Remake: draft (controls.ini:2687-2751); sections right, never opens.
- Mismatches:
  - Never opens; no event hook (MyShellDlg.cpp:9266).
  - Hero model missing (DlgEffigy.cpp:247-250).
  - Eight pose buttons have no action (SetDummyAction 230/210/170/150/160/1/190/200) (DlgEffigy.cpp:275-313).
  - 1364 play/pause has no action and does not swap Button381/Button380 (DlgEffigy.cpp:315-328).
  - 1365/1366 rotate have no action (DlgEffigy.cpp:330-346).
  - 1363 OK has no action (CreateStatuty) (DlgEffigy.cpp:348-352).
  - 1361 Button5 close has no action (DlgEffigy.cpp:270-273).
  - 1667 help no action (DlgEffigy.cpp:254-257).
  - Closing does not clear the right slot (ON_EFFIGY_CLOSE_5065, DlgEffigy.cpp:259-268); no slide-in.

### 242 CDlgLay (place an NPC) - draft only, 6 mismatches
- Client: created DlgMain.cpp:517, drawn 899; CHero::FixMousePlayer posts CMD_OPEN_PLACE_NPC_MODIFY_DIALOG (3drole/Hero.cpp:4871-4875) -> OpenLay (MyShellDlg.cpp:9272-9274; DlgMain.cpp:2504-2509). Dialog36 and seven buttons (DlgLay.cpp:159-211). The engine instead places the NPC at once at the hero's cell (lib.rs placing).
- Remake: draft; never opens.
- Mismatches:
  - No opener; no mouse-following NPC preview (MyShellDlg.cpp:9272-9274).
  - 1367/1368 Button382/383 no action (rotate the preview) (DlgLay.cpp:231-247).
  - 1668 Button0 no action (PutMousePlayer, close) (DlgLay.cpp:249-253).
  - 1671 Button7 no action (DelMousePlayer, close) (DlgLay.cpp:255-259).
  - 1701/1702 only toggle; the client also removes the preview (DlgLay.cpp:224-229).
  - 1667 help no action (DlgLay.cpp:219-222).

### 262 CDlgDice - draft only, 9 mismatches
- Client: created DlgMain.cpp:551, drawn 1078; opened/closed on CMD_DICE_JOIN/QUIT (MyShellDlg.cpp:9369-9374) via OpenDice (DlgMain.cpp:2578-2594), which hides the bag.
- Remake: draft; never opens.
- Mismatches:
  - No opener (MyShellDlg.cpp:9369-9374).
  - 22 bet spots 1393-1414 no action, no money shown (ChipIn, totals) (DlgDice.cpp:199-221, 343+, 620-700).
  - 1417-1419 Button433-435 chip size no action, no cursor change; drawn at (+4,+4) (DlgDice.cpp:233-236, 562-573).
  - 1415 Button430 leave no action (DlgDice.cpp:578-581).
  - 1416 Button431 hide no action; 1633 Button436 always drawn (only after hiding) (DlgDice.cpp:276-277, 583-615).
  - 1420/1421/1632 unbound (own bet, total, chip size) (DlgDice.cpp:239-270, 326-334).
  - 1422 no action (toggle 267) (DlgDice.cpp:727-730; MyShellDlg.cpp:2851-2852).
  - Right-click does not cancel bets (DlgDice.cpp:732-737).
  - Dialog43 should be drawn 2 px lower (DlgDice.cpp:189-195).

### 267 CDlgDiceList - draft only, 3 mismatches
- Client: created DlgMain.cpp:552; toggled by ON_DICE_LIST (MyShellDlg.cpp:2851-2856); filled on CMD_DICE_FLASH_PLAYERLIST (9375-9376).
- Remake: draft.
- Mismatches:
  - Never opened (MyShellDlg.cpp:2851-2852).
  - 1423 unbound (yellow "name bonus" rows) (DlgDiceList.cpp:97-98, 132-146).
  - Dialog44 should be drawn 2 px lower (DlgDiceList.cpp:112-123).

### 284 CDlgOfflineTrainingResult - draft only, 8 mismatches
- Client: created DlgMain.cpp:558, drawn 1108; opened by MsgTrainingInfo -> CMD_ACTION_OFFLINE_TRAINING 3 (3drole/Network/MsgTrainingInfo.cpp:28; MyShellDlg.cpp:8424-8425), requested by MsgAction 126 data 44 (MsgAction.cpp:745-747).
- Remake: draft; window=44 in the engine would open dialog id 44, not 284.
- Mismatches:
  - No ani; the client uses ExerciseBG (DlgOfflineTraining.cpp:291-299).
  - No opener (MyShellDlg.cpp:8424-8425).
  - Texts unbound: 1466 (11009), 1475/1483 (11005/11006), 1468 CP balance, 1472 (11012), 1477 (11010), 1476 (11002) (DlgOfflineTraining.cpp:243-246, 259-289).
  - 1469/1473 Emoney and 1467 ExerciseClock drafted as text; they are static images (DlgOfflineTraining.cpp:236-240).
  - 1465 ExeConfirm no action (continue training) (DlgOfflineTraining.cpp:178, 323-327).
  - 1479 ExeContinue no action (hide, exit to login) (DlgOfflineTraining.cpp:180, 329-335).
  - 1478 no action (hide, message 11003) (DlgOfflineTraining.cpp:181, 317-321).
  - 1180 closes without message 11003 (DlgOfflineTraining.cpp:179, 317-321).

### 285 CDlgOfflineTrainingConfirm - draft only, 5 mismatches
- Client: created DlgMain.cpp:555, drawn 1106; opened by MsgTraining TRAINING_RETURN_READY -> 0x444/0 (3drole/Network/MsgTraining.cpp:72-74; MyShellDlg.cpp:8419-8422).
- Remake: draft.
- Mismatches:
  - No ani; the client uses PublicInfoBG (DlgOfflineTraining.cpp:116-124).
  - No opener (MyShellDlg.cpp:8419-8422).
  - 1531 should show StrRes 11011 at font size 20; 1532 StrRes 11014 with hours/minutes (DlgOfflineTraining.cpp:89-93, 105-114).
  - 1665 no action (start training) (DlgOfflineTraining.cpp:132-136).
  - 1664 no action (hide, post 0x444/6) (DlgOfflineTraining.cpp:138-142).

### 303 CDlgAutoRun5065 (auto-path destinations) - draft only, 7 mismatches
- Client: created DlgMain.cpp:406; toggled by 304 via CMD_AUTORUN_TOGGLE_5065 (MyShellDlg.cpp:8456-8462), cancelled 8464-8470; slides in from the right. path_title, path_target rows (4 frames), path_menutitle/path_bottom submenus, Dialog21 tooltip, title StrRes 11121 (DlgAutoRun5065.cpp:284-512); a row click auto-runs (MyShellDlg.cpp:2905-2950).
- Remake: draft (controls.ini:2268); never opened.
- Mismatches:
  - Never opened (MyShellDlg.cpp:8456-8462).
  - Dialog ani Dialog21 wrong (only the tooltip back) (DlgAutoRun5065.cpp:455-462).
  - 1540 Button150 drawn; the client never draws m_BtnPosition.
  - Title StrRes 11121 missing (DlgAutoRun5065.cpp:70-71).
  - No per-map destination rows (MapDestination.dat), submenus or tooltip (DlgAutoRun5065.cpp:195-238).
  - No auto-run on row click (MyShellDlg.cpp:2905-2950).
  - No slide-in; GUI [0-303] x=1000 puts the draft mostly off-screen.

### 334 CDlgFurnishment5065 (item-bind list) - draft only, 5 mismatches
- Client: created MyShellDlg.cpp:5105; auto-shown on CMD_FURNISHMENT_ADD_5065 (8962-8967; DlgItemLock5065.cpp:237-255). Furnishment_BG, 9 rows of Furnishment_ItemBG + name + Furnishment_BindBtn, title StrRes 11312, 9x1 grid, Button382/383 paging, page text, AllBind, Confirm (DlgItemLock5065.cpp:84-116, 331-359); bind sends MsgItemLock (281-292).
- Remake: draft (controls.ini:3237).
- Mismatches:
  - Never opened (MyShellDlg.cpp:8962-8967).
  - No Furnishment_BG.
  - Names 2210-2218, bind buttons 2270-2278 and row backgrounds missing.
  - No actions (bind, all-bind, prev, next, confirm) (DlgItemLock5065.cpp:281-292).
  - 2071 title, 2279 page text, grid 2201 unbound.

### 380 CDlgQuestList5065 / 381 CDlgQuestTrack5065 - draft only (non-native additions), 7 mismatches
- Client: created MyShellDlg.cpp:5099-5100, commented there as additions (the v6159 port), not 5065 owners; 380 toggled by Alt+Q (4938-4945 -> 8405-8407); 381 drawn every frame for traced quests (1446-1448). Their Task_* skin sections are only in the built stage Control.Ani, not client5065/ani/Control.Ani.
- Remake: drafts (controls.ini:4341, 4370), never opened; the engine draws its own egui quest log (L) and tracker.
- Mismatches:
  - 380 not opened; Alt+Q absent (MyShellDlg.cpp:4938-4945).
  - 380 tabs, scroll buttons, tree rows, detail text and award slots not drafted.
  - 380 Trace/Continue/Quit/Accept/More no actions.
  - Task_* skin missing from the 5065 Control.Ani.
  - 381 not auto-shown (MyShellDlg.cpp:1446-1448).
  - 381 rows and panel not drafted.
  - 381 buttons no actions.

### 215 CDlgHelp - draft only, 2 mismatches
- Client: created MyShellDlg.cpp:979; embedded browser showing Help<n>.htm (DlgHelp.cpp:90-116), opened by every Help button via ON_HELPDLG_SHOW_5065 (MyShellDlg.cpp:2821-2823, 9725-9737).
- Remake: empty draft (controls.ini:3169).
- Mismatches:
  - No help viewer (DlgHelp.cpp:90-116).
  - No Help button (Button4/Button444) anywhere has an action (MyShellDlg.cpp:9725-9737).

### 312 CDlgSchoolList (mentor) - draft only, 9 mismatches
- Client: created DlgMain.cpp:444; opened only by contact-page buttons (ON_CONTACT_PAGE_5065 2, MyShellDlg.cpp:2750-2754); refreshes and queries on show (DlgSchoolList.cpp:1682-1690). Draws Dialogteacher, tip 2066, teacher face 2040, name 2041, offer 2042, invisible detail button 2039, list 2022 (yellow), student buttons 2024-2033 (btn-viewtushun, enabled rows only), 1571 btn-contribute, btn-pangshi 1568 (has teacher) or btn-master 2023, 1569 btn-shoutu, 1570 btn-dismiss, 2061 btn-friend, 2062 btn-Enemy, 2063 Btn-Business (2110-2147); clicks in WindowProc (1708-1735).
- Remake: draft; unreachable.
- Mismatches:
  - Dialogteacher frame missing (DlgSchoolList.cpp:2110).
  - Student buttons 2024-2033 missing.
  - Tip, face, name, offer, list unbound.
  - Follow and Betray both drawn; the client shows one (DlgSchoolList.cpp:2133-2136).
  - No button actions (detail, follow, betray, take-in, dismiss, contribution, friend, enemy, business) (DlgSchoolList.cpp:1708-1735).
  - List click -> 314/315 missing.
  - Disabled states missing (DlgSchoolList.cpp:1836-1874).
  - Child pages 313-316 not hidden on close (HideDetails, DlgSchoolList.cpp:1897-1909).
  - Nothing opens it (MyShellDlg.cpp:2750-2754).

### 313 / 314 / 316 mentor state pages, 315 grand-student list - draft only, 4 mismatches each
- Client: created by 312 (DlgSchoolList.cpp:750, 1785-1787) at GUI x=210 (316 at 357); 313/314/316 draw DialogStateBg, a title image (DialogTeaState / DialogPreState / DialogPrePreState), face, texts (268-288, 460-481, 631-646); 313 btn-bigpangshi, 314 btn-bigdismiss; 315 DialogPre, name 2056, list 2057 (803-813).
- Remake: drafts.
- Mismatches (each):
  - Frame (DialogStateBg / DialogPre) missing.
  - Title 2071 drafted as text, draws nothing.
  - Everything unbound.
  - Betray / Dismiss / list have no action; never opened.

### 317 CDlgMentorNotice5065 / 318 CDlgMentorConfirm5065 - draft only, 4 mismatches each
- Client: created MyShellDlg.cpp:5101-5102, queued from server commands (8768-8797); Dialog15, text 2066, Button150 (+Button151 on 318) (DlgMentorPrompt5065.cpp:18-27, 132-139, 277-285); accept/reject send CMsgMentor5065 (287-299).
- Remake: drafts.
- Mismatches (each):
  - Dialog15 frame missing.
  - Text unbound.
  - No button actions.
  - No server event opens them (MyShellDlg.cpp:8768-8797).

### 319 / 320 / 322 / 323 / 324 mentor contribution pages - missing
- Client: created by 312 (DlgSchoolList.cpp:1047, 1788-1791); frames dssy-lqgxBG / dssy-zxgx / dssy-zxgxphBG / dssy-tdgx / dssy-tdgx1 with claim buttons, tabs, ranking rows, paging, a dropdown (1186-1236, 1604-1631); opened from 325 or 312-1571 via OpenMentorContribution5065 (DlgMain.cpp:2776-2840).
- Remake: no [0-319]..[0-324].
- Mismatches: all five pages missing (DlgSchoolList.cpp:1788-1791).

### 326 CDlgBusiness (trade partners) - draft only, 7 mismatches
- Client: created DlgMain.cpp:445; opened by ON_CONTACT_PAGE_5065 3 (MyShellDlg.cpp:2755-2759); slides in. GeneralBG, title Dialogbusiness-shuxing 2090, list 1680, Add 2070 Btn-AddBusinesser (under 20 partners), 1681 Button5, 1667 Button444, 2084 btn-friend, 2085 btn-Enemy, 1568 btn-teacher, 20 assessment icons 1682-1701 (businesskaocha1-3) with hour tips (DlgBusiness.cpp:996-1052).
- Remake: draft; unreachable.
- Mismatches:
  - GeneralBG frame missing.
  - List unbound.
  - Add no action (ON_BUSINESS_ADD_5065 pick, MyShellDlg.cpp:2789-2795) and not hidden at 20.
  - Friend / enemy / mentor buttons no action (DlgBusiness.cpp:1178-1180).
  - Help no action.
  - 20 assessment icons missing.
  - Row select does not open 327.

### 327 CDlgBusinessState - draft only, 4 mismatches
- Client: created by 326 (DlgBusiness.cpp:968) at (205,65); GeneralShuxingBG1, face, assessment icon, texts, delete Button250, close (184-305).
- Mismatches:
  - Frame missing ([0-327] no ani).
  - Texts and images unbound.
  - 2080 delete no action (DlgBusiness.cpp:311-319).
  - Never opened; overlaps 326 (205 < 242).

### 331 CDlgBusinessTip5065 - draft only, 4 mismatches
- Client: created MyShellDlg.cpp:5097; opened on CMD_PROTECTED_ITEM_TIP / CMD_TRADE_BUDDY_TIP (8409-8415); For_BusinessBG, text 2066, 1668 For_ConfirmBtn, 2194 For_BusinessBtn in mode 1 (DlgBusinessTip5065.cpp:99-104, 143-170).
- Mismatches:
  - Frame missing.
  - Button 1668 missing.
  - 2194 not conditional, no action (ApplyBusiness).
  - No event opens it (MyShellDlg.cpp:8409-8415).

### 332 CDlgBusinessConfirm - draft only, 4 mismatches
- Client: created MyShellDlg.cpp:5104; ShowMode from CMD_BUSINESS_CONFIRM_STATE (9006-9012); frame ApplyBusinessBG / AuditBusinessBG / DisBusinessBG by mode; only enabled buttons drawn; day count BusinessNum frame N-1 (DlgBusiness.cpp:1274-1339).
- Mismatches:
  - No frame.
  - All buttons drawn, none act.
  - 2195 is text, so the BusinessNum image draws nothing.
  - Never opened.

### 333 CDlgBusinessShare5065 - draft only, 3 mismatches
- Client: created MyShellDlg.cpp:5103; opened by SignalListReady (DlgBusiness.cpp:654-660); RegisterBG, per-row ColludeBG/ItemBG/HeadBG, 2x9 Collude checks, names, CancleSBtn, faces, subtitles 2255/2256, page buttons 2257-2260, OK Button555 (467-525, 662-724).
- Mismatches:
  - Only 3 of 72 controls exist.
  - No frame.
  - No actions; never opened.

### 307 CDlgNobilityMain - draft only, 6 mismatches
- Client: created DlgMain.cpp:564; opened only via CMD_NOBILITY_OPEN from a status icon (3dgamemap/StatusTips5065.cpp:487, 604; MyShellDlg.cpp:8654-8666): below level 70 only 309, else 307 + 310. checont_checontribute at (x-5,y-5), texts, donation edit 1565, six rank buttons (King checont_infante/infanta by sex), rank buttons disabled above the hero's rank (DlgNobility.cpp:359-381); close also hides 310 (385); contribute -> 308; help 0x36.
- Mismatches:
  - Nothing opens it (MyShellDlg.cpp:8654-8666).
  - Frame missing.
  - Texts and edit unbound.
  - King art fixed to infante.
  - Rank buttons, contribute, help no action.
  - Close does not hide 310 (DlgNobility.cpp:385).

### 308 CDlgNobilityConfirm - draft only, 4 mismatches
- Client: Dialog15, info 1578, OK/ReInput only in mode 0, Close swaps Button151/Button150 (DlgNobility.cpp:460-500).
- Mismatches:
  - Frame missing.
  - Text unbound.
  - OK / ReInput no action; Close does not send CLOSE_ALL.
  - Never opened.

### 309 CDlgNobilityRanking - draft only, 5 mismatches
- Client: checont_exploitbead, 11 rows (rank, icon, title, name, money), page text 1622, prev/next, donation block at level >= 70 (DlgNobility.cpp:709-744).
- Mismatches:
  - Frame missing.
  - Rows missing (16 controls vs 72 in GUI.ini).
  - Paging no action.
  - Donation block not level-gated; King art not by sex.
  - Never opened.

### 310 CDlgNobilityCompact - draft only, 3 mismatches
- Client: che_checontribute2 at (x-5,y-5), 5 rows, More checont_more -> CMD_NOBILITY_SHOW_RANKING (DlgNobility.cpp:774-786).
- Mismatches:
  - Frame missing.
  - Rows missing (1 vs 21 controls).
  - More no action; never opened.

### 365 CDlgEmotion - ok (dead in 5065), 0 mismatches
- Client: created disabled at (630,565) (DlgMain.cpp:206-211); only shown via ON_DLGEMOTION_OPEN (MyShellDlg.cpp:3553-3565), which nothing in 5065 posts; its skins Dialog85/Button580/Button581 are absent from the 5065 Control.Ani and GUI.ini has no 365.
- Remake: nothing - correct.

### Not used in 5065 (absence is correct)
- 365 CDlgEmotion: created disabled (DlgMain.cpp:206-211); nothing posts ON_DLGEMOTION_OPEN; its skins are absent from the 5065 Control.Ani.
- 345 CDlgTalkTarget and 280 CDlgMsg: compiled, never instantiated (DlgMain.h:237).
- 195 CDlgTitleBar: windowed-mode title strip only (MyShellDlg.cpp:9834-9838); the engine owns the OS window.
- 299 CDlgAntiAddiction5065: renders only for the Chinese version string (MyShellDlg.cpp:60-64, 1459-1462).
- 305 CDlgSetup_Music / 5303 CDlgSetup_Chat: compiled, never instantiated.
- 276 title, 279 IDD_DIALOG_EQUIP, 300 QGood, 301 StoneGoods: not created in 5065 (DlgQuery.cpp:757; DlgEquip.h:39).
- Knightage sub-dialogs 288, 291, 306, 311, 321 and the DlgKnightageBase/BillBoard/Enemy/MemList/State files: excluded from the build (MyShell.vcxproj:98).
- 105 CDlgEnchant: created, but nothing in 5065 posts its open command 0x443 (MsgAction.cpp:729-750) - unreachable in the client too.

## Missing engine features needed by many dialogs

### 1. Window model and dialog lifecycle
- Client-style owner slots (left / middle / right, `CloseCurDlg`, DlgMain.cpp:1410) instead of the rect-overlap rule in `Gui::open`:
  - Opening the shop, warehouse, workbench, booth or lock dialogs must show the bag (DlgMain.cpp:1652-1725, 2408-2496).
  - Shop, warehouse and compose must be refused while trading.
  - Child dialogs (268, 257, 209/270, 313-316, 327, 148, 174, 274) must not close their owner, and must close along with it.
  - Independent HUD strips (140, 188, 191) must not close each other.
- A `close:<id>` action that works on dialogs shown through `when`. Today, `toggle` on a `when` dialog sticks it open (153).
- Esc closes the top dialog. The client's Esc chain is task, then log, then bag plus owner, then the exit confirm (MyShellDlg.cpp:4912-4931, 10855-10889).
- `open=always` for the HUD dialogs (304, 325, 328, 272, 295, 289 with a condition).
- Server events and cursor commands that open dialogs: 158 (all modes), 189, 233/234, 241, 242, 257, 258, 262, 278 (window 2/3), 284/285, 307-310, 317/318, 331-333, 334.
- The 300 ms slide-in, DlgOpen/DlgClose per dialog, and draw offsets (Dialog15 at -5,-5; Dialog24 at -3,-4; Dialog43/44 at +2; Dialog10 at +2).
- Dialogs sized or placed by content:
  - the skill strips right-aligned by count;
  - the portable chest by rows;
  - the NPC talk centred on the real screen;
  - two-state dialogs (272 compact/expanded);
  - modes inside a dialog (133 pages, 158 / 290 / 296 / 240 modes).
- A help viewer (215) and a `help:<topic>` action. Every Button4 / Button444 / Public_HelpSignBtn has no action today.

### 2. Control behaviour
- **Buttons:**
  - a disabled state (no click; frame 2 = disabled for 3-frame skins such as Button195, Button80/83-86, Check62 rather than hover);
  - a latched or selected frame from a binding (trade OK/Agree, tabs in 240/294, the 131 tab checks);
  - a skin chosen by a binding (130-1013 PK mode, 145-1094 channel, 141 Btn1/Btn2, 329-2188, 241 play/pause, 139 colour images, 294 `Pic_*`);
  - flashing (272).
- **Hover tooltips:** HUD meters and checks (Dialog21), items (bag, equipment, shop with price, warehouse, trade), skills, and misc tips.
- **Grids:**
  - drag-and-drop between grids, the map, the quick bar and equipment;
  - right-click use and buy, double-click remove or unequip;
  - paging with an offset;
  - +N, quality-effect and colour overlays, and the suspicious-item "reif" effect;
  - item counts.
- **Lists:** wrapped multi-line rows, hover text colour (NPC answers red), row select and double-click on any list, per-row colour (online / offline), scroll arrows (`[id-1]` / `[id-2]` in GUI.ini), pages.
- **Edits:**
  - readable and writable for any field (only storage.amount, trade.amount, dialogue.typed, chat.text and chat.name today);
  - maximum length, digits only, live thousands commas;
  - password mode taken from data;
  - a background skin (Edit230, Edit150, VipInfo2);
  - multi-line (232, 240, 133-1512);
  - focus on open.
- **Text:** conditional colour (PK, NPC hover), thousands-comma and right-aligned number formatting, and string-table lookups for defaults (StrRes 10075 "none").
- `type=text` with an `ani` should draw the image, or the drafts must say `type=image`. Many drafts have static title images as text (2071 DialogFriend, 1456 Dialog56, 2195 BusinessNum ...).
- New control types:
  - a slider (Setup 1073/1074, chat line slider `Slider0`);
  - a key-combo capture (231);
  - an in-game hero/player model with pose and rotation (135, 241, 258);
  - level-pip images (Weaponlevel5/1/20);
  - an animated ani icon grid (EmotionIco.ani, 274).
- `when` with comparisons (`level>=70`, `profession in 11-15`, `confirm.mode==5`), not only non-zero tests.

### 3. Bindings (data the UI cannot read today)
- **Hero:** XP, stamina (and Blessed), CPs/eMoney, PK mode, nobility rank, battle power and its breakdown, weapon-skill and magic exp % and descriptions, filtered skill lists (magic / Kongfu / XP, enabled flag, transform-aware), the selected right-click skill, dead / flying / transformed / XP-full states, worn slots 7/9 (treasure, mount).
- **Quick bar:** contents of the 10 slots.
- **Shop and storage:** shop currency (silver / CP), page and page count for shop, warehouse and trade grids, the partner's level, CPs and nobility in a trade.
- **Workbench:** typed slots (main / minor / gem / socket), the slot-enabled state, gems already in an item, more than 5 slots (the oven needs 20).
- **Team:** each member's face id, the selected member.
- **Guild:** contribution, rank title, announcement, ally/enemy rows, member rows with level and online flag, data from the server (today `guild.*` reads only the local sim).
- **Contacts:** friends, enemies, trade partners, mentor/students, nobility ranking, blacklist, recent chat targets, bulletin board, pigeon queue.
- **Other player's equipment** (258) and the battle-power detail rows (287).
- **Confirm prompt:** mode, text, amount.
- **Radar:** visible state and mode.

### 4. Actions and game inputs
- **Actions:**
  - `select_skill:N` (choose without casting, then hide the strip);
  - `emote:<id>` (140, quick bar);
  - `pk_mode:<free|monster|team|arrest>` (191);
  - radar toggle, overview and full; screen-move toggle; view-equipment pick (130-1015/1023/1016);
  - confirm OK / Cancel per mode; drop money with an amount; guild donate, notice, kick and join pick; team follow, selected-member kick, member select;
  - friend add / remove / whisper / leave word; enemy remove; trade-partner add / remove; mentor follow / take-in / betray / dismiss / claim; nobility donate / page;
  - booth open, add, take back, buy, cry-out; leave-word send and read; CP Mall open; CP withdraw; CP buy with an amount; portable chest;
  - repair mode; change face; improve / level-up (Input::UseOn from the workbench); refine (oven); place-NPC preview with rotate, confirm and cancel; dice bets;
  - offline training start, continue and exit; revive here or in town; land; abort transform;
  - setup volumes / music / show-names / shadow / screen mode / exit / delete role; hotkey change, revert and delete;
  - auto-run to a destination; item lock apply / cancel / reapply.
- **c3_game inputs that don't exist:** PK mode, emote, team follow, friend / enemy / mentor / business / nobility, trade reject, repair, booth, leave words, CP withdraw / buy-with-amount, portable chest, item lock, delete role, path-to-NPC auto-run.

### 5. Chat
The chat bar (145) and the channel menu (174) are now hand-written. Commit 67d93e5 added `ChatChannel {Local, Whisper, Team, Guild}`. What is still missing:
- **Channels:**
  - Friend (2009) and Service (2014, with a 20 s cooldown) can be picked but are refused on send.
  - Broadcast/pigeon (2500): level 50, 5 CPs, 80 bytes, queue, urgent/first, banner 295, board 294, prompts 296.
  - System (2005, a top-left area with a 15 s timeout that flashes on repeat), GM (2011, a mid-screen area), action (2002), ghost (2013), leave-word (2110) and the boards (2201-2205).
  - `Event::Chat` needs the receiver, an emotion, a time, a colour and a style (scroll / flash / blast).
- **Log:**
  - The client's g_objGameMsg replaces the egui box: per-channel prefixes, "A to B says:" format, timestamps, wrapping to the screen width from screenH-130.
  - Separate system and GM areas.
  - An adjustable line count (Slider0, default 6, saved).
  - Per-channel colours and on/off filters (139, saved).
  - Inline `#NN` emoticons; overhead speech lines; Alt+click on names; a log viewer (218).
- **Input line:** an 88-character limit, a 20-line Up/Down history, Ctrl+Up/Down to cycle channels, Ctrl+Left/Right through the 5 recent targets, a 500 ms flood guard, Shift+click a player to target them, "chat with" routes from the friend, guild, team and business dialogs, and recent targets (148) with persistence.
- **Social filters:** blacklist (239) add, delete and filtering; whispers to offline friends turned into leave-words (293).
- **Private sessions** (2232-2234): per-conversation windows, a tray of flashing face/team icons, the conceal toggle, a "more" list, an emotion popup and peer details.

### 6. Keys
- **Missing:**
  - F1-F10 quick bar (MyShellDlg.cpp:4805-4818);
  - F12 screenshot;
  - Shift held shows all names;
  - Ctrl chat cycling;
  - Esc chain;
  - the 70 user-assignable hotkey actions: emotes 10-19, status pages 20-22, friends 30/31, PK cycle 40, guild 41, trade 42, skills 43, bag 44, setup 45, team 50-55, run / map / screen-move / clear 60-63, chat 70-72 (MyShellDlg.cpp:10144-10300).
- **Engine-only keys** (B, L, Tab, R, Q/E, 1-8, F, Space, WASD) have no client equivalent.
