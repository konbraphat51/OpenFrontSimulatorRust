// Generates Config.json: upstream Config methods over several game configs and input grids.
import { bits, upstream } from "./Common.mts";

const { Config } = await upstream("core/configuration/Config");
const G = await upstream("core/game/Game");
const { UnitType, PlayerType, TerrainType } = G;

const base = {
  gameMap: "World", difficulty: "Medium", donateGold: false, donateTroops: false,
  gameType: "Public", gameMode: "Free For All", gameMapSize: "Normal",
  nations: "default", bots: 100, infiniteGold: false, infiniteTroops: false,
  instantBuild: false, randomSpawn: false,
};
const cfgs: any[] = [
  base,
  { ...base, difficulty: "Easy", instantBuild: true, gameType: "Singleplayer" },
  { ...base, difficulty: "Hard", randomSpawn: true, nations: "disabled", goldMultiplier: 2.5, startingGold: 1000000,
    overtime: { enabled: true, startMinutes: 5 }, disabledUnits: ["Transport", "Port"], customAllianceDuration: 7,
    spawnImmunityDuration: 200, playerTeams: "Duos", doomsdayClock: { enabled: true, speed: "fast" } },
  { ...base, difficulty: "Impossible", infiniteGold: true, infiniteTroops: true, customAllianceDuration: 0,
    disableAlliances: false, waterNukes: true, nations: 12, playerTeams: 4, ranked: undefined },
  { ...base, hostCheats: { infiniteGold: true, infiniteTroops: true, goldMultiplier: 3, startingGold: 500 },
    goldMultiplier: null, startingGold: null, disableAlliances: true, disableNavMesh: true, waterNukes: null },
];

const types = ["HUMAN", "BOT", "NATION"];
const allUnits = Object.values(UnitType) as string[];
const out: any[] = [];
const f = (x: number) => ({ f: bits(x) });
const bi = (x: bigint) => ({ i: x.toString() });
const mkPlayer = (type: string, creator: boolean, tiles: number, troops: number, cityLevels: number[], owned: Record<string, number>, constructed: Record<string, number>, gold = 0n) => ({
  type: () => type, isLobbyCreator: () => creator, numTilesOwned: () => tiles, troops: () => troops,
  units: (t: string) => (t === UnitType.City ? cityLevels.map((l, i) => ({ isUnderConstruction: () => i % 5 === 4, level: () => l })) : []),
  unitsOwned: (t: string) => owned[t] ?? 0, unitsConstructed: (t: string) => constructed[t] ?? 0, gold: () => gold,
});

cfgs.forEach((cfgJson, ci) => {
  const c = new Config(cfgJson, null, false);
  const rec = (fn: string, args: any[], res: () => any) => {
    let o: any;
    try { o = res(); } catch (e: any) { o = { err: String(e.message) }; }
    out.push({ cfg: ci, fn, args, out: o });
  };
  const simple = ["traitorDefenseDebuff", "traitorSpeedDebuff", "traitorDuration", "teamLandShareWinThresholdTenths", "spawnImmunityDuration",
    "nationSpawnImmunityDuration", "cityTroopIncrease", "msPerTick", "SAMCooldown", "SiloCooldown", "defensePostRange", "defensePostDefenseBonus",
    "defensePostSpeedBonus", "spawnNations", "bots", "instantBuild", "disableNavMesh", "disableAlliances", "waterNukes", "isRandomSpawn",
    "infiniteGold", "donateGold", "infiniteTroops", "donateTroops", "goldMultiplier", "trainStationMinRange", "trainStationMaxRange",
    "railroadMaxSize", "donateCooldown", "embargoAllCooldown", "deletionMarkDuration", "deleteUnitCooldown", "emojiMessageDuration",
    "emojiMessageCooldown", "quickChatCooldown", "targetDuration", "targetCooldown", "allianceRequestDuration", "allianceRequestCooldown",
    "allianceDuration", "temporaryEmbargoDuration", "minDistanceBetweenPlayers", "armyLimitWarningThreshold", "boatMaxNumber",
    "numSpawnPhaseTurns", "numBots", "warshipShellLifetime", "radiusPortSpawn", "tradeShipShortRangeDebuff", "mirvNormalizeTargetTicks",
    "nukeAllianceBreakThreshold", "defaultNukeTargetableRange", "defaultSamRange", "maxSamRange", "samUpgradeDuration", "defaultSamMissileSpeed",
    "structureMinDist", "shellLifetime", "warshipPatrolRange", "warshipTargettingRange", "warshipShellAttackRate", "warshipDockingRange",
    "warshipPortHealingBonusPerLevel", "warshipRetreatHealthPercent", "warshipPassiveHealing", "warshipPassiveHealingRange",
    "warshipPortSwitchThreshold", "warshipMaxVeterancy", "warshipVeterancyHealthBonus", "warshipVeterancyShellDamageBonus",
    "warshipVeterancyTransportKills", "warshipVeterancyTradeCaptures", "defensePostShellAttackRate", "safeFromPiratesCooldownMax",
    "defensePostTargettingRange", "allianceExtensionPromptOffset", "hasExtendedSpawnImmunity"];
  for (const name of simple) rec(name, [], () => { const v = c[name](); return typeof v === "number" ? f(v) : v; });
  rec("playerTeams", [], () => c.playerTeams());
  rec("doomsdayClockConfig", [], () => Object.fromEntries(Object.entries(c.doomsdayClockConfig()).map(([k, v]) => [k, typeof v === "number" ? f(v as number) : v])));
  rec("overtimeConfig", [], () => Object.fromEntries(Object.entries(c.overtimeConfig()).map(([k, v]) => [k, typeof v === "number" ? f(v as number) : v])));
  for (const u of allUnits) {
    rec("isUnitDisabled", [u], () => c.isUnitDisabled(u));
    rec("unitInfo", [u], () => { const i = c.unitInfo(u); return { maxHealth: i.maxHealth ?? null, damage: i.damage ?? null, cd: i.constructionDuration ?? null, up: i.upgradable ?? null }; });
    for (const t of ["nukeMagnitudes", "nukeSpeed"]) rec(t, [u], () => { const v = c[t](u); return typeof v === "number" ? f(v) : { inner: f(v.inner), outer: f(v.outer) }; });
  }
  for (const t of types) for (const creator of [false, true]) {
    rec("startingGold", [t, creator], () => bi(c.startingGold({ playerType: t, isLobbyCreator: creator })));
    rec("startManpower", [t, creator], () => f(c.startManpower({ playerType: t, isLobbyCreator: creator })));
    rec("goldAdditionRate", [t, creator], () => bi(c.goldAdditionRate(mkPlayer(t, creator, 0, 0, [], {}, {}))));
    for (const tiles of [1, 17, 2500, 123456]) for (const levels of [[], [1, 2, 3, 1, 4, 2, 1]]) for (const troops of [0, 1000, 77777.5, 5e6]) {
      const p = () => mkPlayer(t, creator, tiles, troops, levels, {}, {});
      const sum = levels.filter((_, i) => i % 5 !== 4).reduce((a, b) => a + b, 0);
      const args = [t, creator, f(tiles), f(sum), f(troops)];
      rec("maxTroops", args, () => f(c.maxTroops(p())));
      rec("troopIncreaseRate", args, () => f(c.troopIncreaseRate(p())));
    }
    for (const rel of ["self", "team", "ally", "other"]) for (const cities of [0, 5, 9, 10, 15, 40])
      rec("trainGold", [rel, f(cities), creator], () => bi(c.trainGold(rel, cities, mkPlayer(t, creator, 0, 0, [], {}, {}))));
    for (const dist of [0, 10, 300, 750.5, 2000]) rec("tradeShipGold", [f(dist), creator], () => bi(c.tradeShipGold(dist, mkPlayer(t, creator, 0, 0, [], {}, {}))));
    for (const u of allUnits) for (const owned of [0, 1, 4, 12, 40]) for (const extra of [0, 3]) {
      const ownedMap: Record<string, number> = {}; const consMap: Record<string, number> = {};
      for (const k of allUnits) { ownedMap[k] = owned; consMap[k] = owned + 2; }
      ownedMap[UnitType.Port] = owned; consMap[UnitType.Factory] = Math.max(0, owned - 1);
      const pl = mkPlayer(t, creator, 0, 0, [], ownedMap, consMap);
      const game = { stats: () => ({ numMirvsLaunched: () => BigInt(owned) }) };
      rec("unitCost", [u, t, creator, owned, extra], () => bi(c.unitInfo(u).cost(game, pl, extra)));
    }
    for (const g of [0n, 1n, 7n, 123456789n]) rec("conquerGoldAmount", [t, g.toString()], () => bi(c.conquerGoldAmount(mkPlayer(t, creator, 0, 0, [], {}, {}, g))));
  }
  for (const n of [0, 1, 10, 35, 100, 300, 560, 810, 900, 1200]) {
    rec("trainSaturation", [f(n)], () => f(c.trainSaturation(n)));
    rec("tradeShipSaturation", [f(n)], () => f(c.tradeShipSaturation(n)));
    for (const x of [0, 3, 12]) {
      rec("trainSpawnRate", [f(x), f(n)], () => f(c.trainSpawnRate(x, n)));
      rec("tradeShipSpawnRate", [f(x), f(n)], () => f(c.tradeShipSpawnRate(x, n)));
    }
  }
  for (const s of [0, 59.9, 299, 300, 301.5, 900, 1800, 7200]) rec("percentageTilesOwnedToWin", [f(s)], () => f(c.percentageTilesOwnedToWin(s)));
  for (const t of types) for (const tr of [0, 100.5, 999999]) {
    rec("boatAttackAmount", [f(tr)], () => f(c.boatAttackAmount(mkPlayer(t, false, 0, tr, [], {}, {}), null)));
    rec("attackAmount", [t, f(tr)], () => f(c.attackAmount(mkPlayer(t, false, 0, tr, [], {}, {}), null)));
    rec("defaultDonationAmount", [f(tr)], () => f(c.defaultDonationAmount(mkPlayer(t, false, 0, tr, [], {}, {}))));
  }
  for (const v of [0, 4, 9, 10, 100, 1000]) rec("proximityBonusPortsNb", [f(v)], () => f(c.proximityBonusPortsNb(v)));
  for (const l of [0, 1, 2, 3, 5, 10]) rec("samRange", [f(l)], () => f(c.samRange(l)));
  for (const st of [undefined, { startRange: 70, targetLevel: 2, duration: 45 }, { upgradeStartTick: 100, startRange: 70, targetLevel: 2, duration: 45 }]) for (const tick of [90, 100, 120, 145, 500]) {
    const sam = { samLauncherState: () => st, level: () => 1 };
    rec("dynamicSamRange", [st ? { ...st, hasStart: st.upgradeStartTick !== undefined } : null, f(tick)], () => f(c.dynamicSamRange(sam, tick)));
  }
  for (const nt of ["Atom Bomb", "Hydrogen Bomb", "MIRV Warhead"]) for (const humans of [0, 10, 5000, 1e6]) for (const tiles of [0, 1, 100]) for (const mt of [1000, 1e5, 3e6])
    rec("nukeDeathFactor", [nt, f(humans), f(tiles), f(mt)], () => f(c.nukeDeathFactor(nt, humans, tiles, mt)));
  for (const fr of [0, 0.25, 1]) rec("falloutDefenseModifier", [f(fr)], () => f(c.falloutDefenseModifier(fr)));
  // attackLogic grid.
  const terrains = [TerrainType.Plains, TerrainType.Highland, TerrainType.Mountain, TerrainType.Impassable, TerrainType.Ocean];
  if (ci === 0 || ci === 2) for (const terrain of terrains) for (const at of types) for (const aTiles of [1, 300000, 2e6]) for (const atroops of [100, 5e6]) for (const bs of [37.5]) for (const post of [false, true]) for (const fall of [null, 0.4]) {
    if ((terrain === TerrainType.Impassable || terrain === TerrainType.Ocean) && (at !== "HUMAN" || aTiles !== 1 || atroops !== 100 || post || fall !== null)) continue;
    const defs: any[] = [null];
    for (const dt of types) for (const dTiles of [1, 4000, 1e6]) for (const dTroops of [10, 30000, 4e6]) for (const flags of [[false, false], [true, false], [false, true]])
      if ((dt === "HUMAN" || flags[1] || dTiles !== 4000 || dTroops !== 30000) && !(post && flags[0])) defs.push({ type: dt, numTiles: dTiles, troops: dTroops, isTraitor: flags[0], isDisconnectedTeammate: flags[1] });
    // Keep the grid bounded: sample every 7th defender.
    defs.filter((_, i) => i % 11 === 0).forEach((defender) => {
      const input = { terrain, attackTroops: atroops, attacker: { type: at, numTiles: aTiles }, defender, defenderHasDefensePost: post, falloutRatio: fall, borderSize: bs };
      rec("attackLogic", [terrain, at, f(aTiles), f(atroops), defender && { type: defender.type, numTiles: f(defender.numTiles), troops: f(defender.troops), isTraitor: defender.isTraitor, isDisconnectedTeammate: defender.isDisconnectedTeammate }, post, fall === null ? null : f(fall), f(bs)],
        () => { const r = c.attackLogic(input); return [f(r.attackerTroopLoss), f(r.defenderTroopLoss), f(r.tickFraction)]; });
    });
  }
});
console.log(JSON.stringify({ cfgs: cfgs.map((c) => JSON.parse(JSON.stringify(c))), out }));
