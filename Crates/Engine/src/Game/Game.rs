//! Port of the shared enums and value types of src/core/game/Game.ts (and the map list of Maps.gen.ts).
//!
//! Skipped as client-only or owned by later phases: `MessageType` and categories, `PlayerView`
//! related types, update/record shapes, and the `Player`/`Unit`/`Game` interfaces themselves
//! (their Rust form is `GameImpl`/`PlayerImpl`/`UnitImpl` in P2).

use crate::Core::Util::format_player_display_name;

/// Player identifier string (`PlayerInfo.id`).
pub type PlayerID = String;
/// Game tick counter.
pub type Tick = u32;
/// Gold amount (`bigint` upstream); `i64` covers every reachable value.
pub type Gold = i64;
/// Team name (`"Red"`, `"Bot"`, ...).
pub type Team = String;

/// Defines a fieldless enum whose variants map to exact upstream string values.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $value:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($(#[$vmeta])* $variant),*
        }

        impl $name {
            /// Every variant in upstream declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            /// The exact upstream string value.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $value),*
                }
            }

            /// Parses the exact upstream string value.
            #[allow(clippy::should_implement_trait)]
            pub fn from_str(s: &str) -> Option<Self> {
                match s {
                    $($value => Some($name::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

string_enum! {
    /// Bot and nation difficulty.
    Difficulty {
        Easy = "Easy",
        Medium = "Medium",
        Hard = "Hard",
        Impossible = "Impossible",
    }
}

string_enum! {
    /// Where a game was created.
    GameType {
        Singleplayer = "Singleplayer",
        Public = "Public",
        Private = "Private",
    }
}

string_enum! {
    /// Free-for-all or team game.
    GameMode {
        FFA = "Free For All",
        Team = "Team",
    }
}

string_enum! {
    /// Ranked queue kind.
    RankedType {
        OneVOne = "1v1",
        TwoVTwo = "2v2",
    }
}

string_enum! {
    /// Compact (reduced) or normal map size.
    GameMapSize {
        Compact = "Compact",
        Normal = "Normal",
    }
}

string_enum! {
    /// Unit kinds, in upstream declaration order.
    UnitType {
        TransportShip = "Transport",
        Warship = "Warship",
        Shell = "Shell",
        SAMMissile = "SAMMissile",
        Port = "Port",
        AtomBomb = "Atom Bomb",
        HydrogenBomb = "Hydrogen Bomb",
        TradeShip = "Trade Ship",
        MissileSilo = "Missile Silo",
        DefensePost = "Defense Post",
        SAMLauncher = "SAM Launcher",
        City = "City",
        MIRV = "MIRV",
        MIRVWarhead = "MIRV Warhead",
        Train = "Train",
        Factory = "Factory",
    }
}

string_enum! {
    /// Train car kinds.
    TrainType {
        Engine = "Engine",
        TailEngine = "TailEngine",
        Carriage = "Carriage",
    }
}

string_enum! {
    /// Kind of player.
    PlayerType {
        Bot = "BOT",
        Human = "HUMAN",
        Nation = "NATION",
    }
}

string_enum! {
    /// Doomsday clock preset speeds.
    DoomsdayClockSpeed {
        Slow = "slow",
        Normal = "normal",
        Fast = "fast",
        VeryFast = "veryfast",
    }
}

/// `Nukes` group: units that count as nukes.
pub const NUKES: &[UnitType] = &[
    UnitType::AtomBomb,
    UnitType::HydrogenBomb,
    UnitType::MIRVWarhead,
    UnitType::MIRV,
];

/// `BuildableAttacks` group.
pub const BUILDABLE_ATTACKS: &[UnitType] = &[
    UnitType::AtomBomb,
    UnitType::HydrogenBomb,
    UnitType::MIRV,
    UnitType::Warship,
];

/// `Structures` group.
pub const STRUCTURES: &[UnitType] = &[
    UnitType::City,
    UnitType::DefensePost,
    UnitType::SAMLauncher,
    UnitType::MissileSilo,
    UnitType::Port,
    UnitType::Factory,
];

/// `BuildMenus` group: structures then buildable attacks.
pub const BUILD_MENUS: &[UnitType] = &[
    UnitType::City,
    UnitType::DefensePost,
    UnitType::SAMLauncher,
    UnitType::MissileSilo,
    UnitType::Port,
    UnitType::Factory,
    UnitType::AtomBomb,
    UnitType::HydrogenBomb,
    UnitType::MIRV,
    UnitType::Warship,
];

/// `PlayerBuildable` group: build menus plus transport ships.
pub const PLAYER_BUILDABLE: &[UnitType] = &[
    UnitType::City,
    UnitType::DefensePost,
    UnitType::SAMLauncher,
    UnitType::MissileSilo,
    UnitType::Port,
    UnitType::Factory,
    UnitType::AtomBomb,
    UnitType::HydrogenBomb,
    UnitType::MIRV,
    UnitType::Warship,
    UnitType::TransportShip,
];

impl UnitType {
    /// True for members of [`NUKES`].
    pub fn is_nuke(self) -> bool {
        NUKES.contains(&self)
    }

    /// True for members of [`STRUCTURES`].
    pub fn is_structure(self) -> bool {
        STRUCTURES.contains(&self)
    }
}

/// Largest bulk-purchase amount an intent may carry.
pub const MAX_UPGRADE_AMOUNT: u32 = 50;
/// Bulk steps for nukes.
pub const NUKE_BULK_STEPS: [u32; 2] = [2, 5];
/// Bulk steps for structures.
pub const STRUCTURE_BULK_STEPS: [u32; 2] = [5, 10];

/// Upstream `AllPlayers` marker.
pub const ALL_PLAYERS: &str = "AllPlayers";

/// Team preset names.
pub const DUOS: &str = "Duos";
/// Team preset names.
pub const TRIOS: &str = "Trios";
/// Team preset names.
pub const QUADS: &str = "Quads";
/// Team preset name for humans against nations.
pub const HUMANS_VS_NATIONS: &str = "Humans Vs Nations";

/// `ColoredTeams` in upstream key order (key equals value).
pub const COLORED_TEAMS: &[&str] = &[
    "Red", "Blue", "Teal", "Purple", "Yellow", "Orange", "Green", "Bot", "Humans", "Nations",
];

/// `TeamCountConfig`: a plain team count or a named preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamCountConfig {
    /// Number of teams.
    Count(u32),
    /// Teams of two.
    Duos,
    /// Teams of three.
    Trios,
    /// Teams of four.
    Quads,
    /// Humans versus nations.
    HumansVsNations,
}

/// Diplomatic relation values (numeric like upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Relation {
    /// 0
    Hostile = 0,
    /// 1
    Distrustful = 1,
    /// 2
    Neutral = 2,
    /// 3
    Friendly = 3,
}

/// Terrain classification (numeric like upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TerrainType {
    /// 0
    Plains = 0,
    /// 1
    Highland = 1,
    /// 2
    Mountain = 2,
    /// 3
    Ocean = 3,
    /// 4
    Impassable = 4,
}

/// Map cell coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
}

impl Cell {
    /// Creates a cell.
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Static data of a unit type that is not its cost (see `Config::unit_cost`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UnitInfo {
    /// Hit points.
    pub max_health: Option<f64>,
    /// Damage dealt (shells).
    pub damage: Option<f64>,
    /// Ticks to construct; 0 with instant build.
    pub construction_duration: Option<u32>,
    /// Whether the structure can be upgraded.
    pub upgradable: Option<bool>,
}

/// Port of `SamLauncherState`: an in-progress SAM upgrade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamLauncherState {
    /// Tick the upgrade started, when one is running.
    pub upgrade_start_tick: Option<f64>,
    /// Range when the upgrade started.
    pub start_range: f64,
    /// Level after the upgrade.
    pub target_level: f64,
    /// Upgrade length in ticks.
    pub duration: f64,
}

/// Port of `PlayerInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerInfo {
    /// Username or tribe name.
    pub name: String,
    /// Player kind.
    pub player_type: PlayerType,
    /// Client id; `None` for tribes.
    pub client_id: Option<String>,
    /// Player id string.
    pub id: PlayerID,
    /// Whether this player created the lobby.
    pub is_lobby_creator: bool,
    /// Clan tag.
    pub clan_tag: Option<String>,
    /// Friends' client ids (team assignment input).
    pub friends: Vec<String>,
    /// Server-pinned team slot.
    pub team_index: Option<u32>,
    /// Nation flag code.
    pub nation_flag: Option<String>,
    /// `[clan] name` or `name`.
    pub display_name: String,
}

impl PlayerInfo {
    /// Creates a player info and derives the display name.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        player_type: PlayerType,
        client_id: Option<String>,
        id: PlayerID,
        is_lobby_creator: bool,
        clan_tag: Option<String>,
        friends: Vec<String>,
        team_index: Option<u32>,
        nation_flag: Option<String>,
    ) -> Self {
        let display_name = format_player_display_name(&name, clan_tag.as_deref());
        Self {
            name,
            player_type,
            client_id,
            id,
            is_lobby_creator,
            clan_tag,
            friends,
            team_index,
            nation_flag,
            display_name,
        }
    }
}

/// Port of `Nation`: a map nation with its optional spawn cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Nation {
    /// Spawn cell from the map manifest.
    pub spawn_cell: Option<Cell>,
    /// Player info of the nation.
    pub player_info: PlayerInfo,
}

string_enum! {
    /// Map names (wire values) generated from the upstream `Maps.gen.ts`, in declaration order.
    GameMapType {
        Achiran = "Achiran",
        Aegean = "Aegean",
        Africa = "Africa",
        Alps = "Alps",
        AmazonRiver = "Amazon River",
        Antarctica = "Antarctica",
        ArchipelagoSea = "ArchipelagoSea",
        Arctic = "Arctic",
        Asia = "Asia",
        Australia = "Australia",
        Baikal = "Baikal",
        BaikalNukeWars = "Baikal Nuke Wars",
        BajaCalifornia = "Baja California",
        Balkans = "Balkans",
        Balkhash = "Balkhash",
        Baltics = "Baltics",
        BeringSea = "Bering Sea",
        BeringStrait = "Bering Strait",
        BetweenTwoSeas = "Between Two Seas",
        BlackSea = "Black Sea",
        BosphorusStraits = "Bosphorus Straits",
        BranchingPaths = "Branching Paths",
        Britannia = "Britannia",
        BritanniaClassic = "Britannia Classic",
        CapeCod = "Cape Cod",
        Caribbean = "Caribbean",
        CaspianSea = "Caspian Sea",
        Caucasus = "Caucasus",
        CentralAmerica = "Central America",
        ChannelIslands = "Channel Islands",
        China = "China",
        ChoppingBlock = "Chopping Block",
        ClearwaterLakes = "Clearwater Lakes",
        Conakry = "Conakry",
        Crimea = "Crimea",
        DanishStraits = "Danish Straits",
        DeglaciatedAntarctica = "Deglaciated Antarctica",
        Didier = "Didier",
        DidierFrance = "Didier France",
        Dyslexdria = "Dyslexdria",
        EastAsia = "East Asia",
        Europe = "Europe",
        EuropeClassic = "Europe Classic",
        FalklandIslands = "Falkland Islands",
        FaroeIslands = "Faroe Islands",
        FingerLakes = "Finger Lakes",
        FourIslands = "Four Islands",
        France = "France",
        GatewayToTheAtlantic = "Gateway to the Atlantic",
        Germany = "Germany",
        GiantWorldMap = "Giant World Map",
        GreatLakes = "Great Lakes",
        GulfOfGuinea = "Gulf Of Guinea",
        GulfOfMexico = "Gulf Of Mexico",
        GulfOfStLawrence = "Gulf of St. Lawrence",
        Halkidiki = "Halkidiki",
        Hawaii = "Hawaii",
        Hecatestrait = "Hecate Strait",
        HongKong = "Hong Kong",
        Iceland = "Iceland",
        IndianSubcontinent = "Indian Subcontinent",
        IrishSea = "Irish Sea",
        Italia = "Italia",
        Japan = "Japan",
        JuanDeFucaStrait = "Juan De Fuca Strait",
        Korea = "Korea",
        Labyrinth = "Labyrinth",
        LasVegasStrip = "Las Vegas Strip",
        Lemnos = "Lemnos",
        Levant = "Levant",
        Lisbon = "Lisbon",
        LosAngeles = "Los Angeles",
        Luna = "Luna",
        Manicouagan = "Manicouagan",
        MareNostrum = "Mare Nostrum",
        Mars = "Mars",
        Mena = "Mena",
        MiddleEast = "Middle East",
        MilkyWay = "MilkyWay",
        MississippiRiver = "Mississippi River",
        Montreal = "Montreal",
        MoreThanLuck = "More Than Luck",
        NewYorkCity = "New York City",
        NileDelta = "Nile Delta",
        NorthAmerica = "North America",
        NorthwestPassage = "Northwest Passage",
        Oceania = "Oceania",
        Onion = "Onion",
        Pangaea = "Pangaea",
        Passage = "Passage",
        Pluto = "Pluto",
        QingChina = "Qing China",
        Russia = "Russia",
        SanFrancisco = "San Francisco",
        Scandinavia = "Scandinavia",
        Sierpinski = "Sierpinski",
        Sol = "Sol",
        SouthAmerica = "South America",
        SoutheastAsia = "SoutheastAsia",
        StraitOfGibraltar = "Strait of Gibraltar",
        StraitOfHormuz = "Strait of Hormuz",
        StraitOfMalacca = "Strait Of Malacca",
        Surrounded = "Surrounded",
        Svalmel = "Svalmel",
        TaiwanStrait = "Taiwan Strait",
        TheBox = "The Box",
        TierraDelFuego = "Tierra Del Fuego",
        Titan = "Titan",
        Tourney1 = "Tourney 2 Teams",
        Tourney2 = "Tourney 3 Teams",
        Tourney3 = "Tourney 4 Teams",
        Tourney4 = "Tourney 8 Teams",
        TradersDream = "Traders Dream",
        TwoLakes = "Two Lakes",
        UnitedStates = "United States",
        Venice = "Venice",
        Vietnam = "Vietnam",
        WarshipWarship = "Warship Warship",
        World = "World",
        WorldInverted = "World Inverted",
        YangtzeRiver = "Yangtze River",
        YellowSea = "Yellow Sea",
        Yenisei = "Yenisei",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_strings_round_trip() {
        for u in UnitType::ALL {
            assert_eq!(UnitType::from_str(u.as_str()), Some(*u));
        }
        assert_eq!(UnitType::AtomBomb.as_str(), "Atom Bomb");
        assert_eq!(GameMode::FFA.as_str(), "Free For All");
        assert_eq!(GameMapType::AmazonRiver.as_str(), "Amazon River");
        assert_eq!(GameMapType::ALL.len(), 123);
        assert_eq!(GameMapType::from_str("nope"), None);
        assert_eq!(PlayerType::from_str("NATION"), Some(PlayerType::Nation));
    }

    #[test]
    fn groups_compose() {
        assert_eq!(
            BUILD_MENUS.len(),
            STRUCTURES.len() + BUILDABLE_ATTACKS.len()
        );
        assert!(UnitType::MIRVWarhead.is_nuke());
        assert!(!UnitType::Warship.is_structure());
    }
}
