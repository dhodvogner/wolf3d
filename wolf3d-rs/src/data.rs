use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

const MAP_HEADER_COUNT: usize = 100;
const MAP_PLANES: usize = 3;
const PLAYER_SPAWN_MIN: u16 = 19;
const PLAYER_SPAWN_MAX: u16 = 22;
const NEAR_TAG: u8 = 0xA7;
const FAR_TAG: u8 = 0xA8;
const AREA_TILE: u16 = 107;
const AMBUSH_TILE: u16 = 106;

#[derive(Debug, Clone)]
pub struct GameAssets {
    pub map: TileMap,
    pub wall_textures: Vec<WallTexture>,
    pub source_extension: String,
}

#[derive(Debug, Clone)]
pub struct TileMap {
    pub width: usize,
    pub height: usize,
    pub walls: Vec<u16>,
    pub info: Vec<u16>,
    pub player_start: PlayerStart,
    pub doors: Vec<DoorSpawn>,
    pub enemies: Vec<EnemySpawn>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerStart {
    pub x: f32,
    pub y: f32,
    pub heading: f32,
}

#[derive(Debug, Clone)]
pub struct WallTexture {
    pub texels: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct DoorSpawn {
    pub x: usize,
    pub y: usize,
    pub vertical: bool,
    pub lock: u8,
    pub tile: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct EnemySpawn {
    pub kind: EnemyKind,
    pub x: f32,
    pub y: f32,
    pub patrolling: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum EnemyKind {
    Guard,
    Officer,
    Ss,
    Dog,
    Mutant,
}

#[derive(Debug)]
pub enum DataError {
    NoSupportedDataSet(PathBuf),
    Io { path: PathBuf, message: String },
    InvalidFormat(String),
}

impl Display for DataError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::NoSupportedDataSet(path) => write!(
                f,
                "no supported Wolf3D data set found in {} (expected MAPHEAD/GAMEMAPS/VSWAP with extension WL6/WL1/SDM/SOD)",
                path.display()
            ),
            DataError::Io { path, message } => {
                write!(f, "failed to read {}: {message}", path.display())
            }
            DataError::InvalidFormat(message) => write!(f, "invalid game data format: {message}"),
        }
    }
}

impl std::error::Error for DataError {}

#[derive(Debug, Clone, Copy)]
struct MapHeader {
    plane_start: [u32; MAP_PLANES],
    plane_len: [u16; MAP_PLANES],
    width: u16,
    height: u16,
}

#[derive(Debug, Clone)]
struct DataSetFiles {
    extension: String,
    maphead: PathBuf,
    gamemaps: PathBuf,
    vswap: PathBuf,
}

pub fn load_assets(data_dir: &Path, map_index: usize) -> Result<GameAssets, DataError> {
    let data_set = detect_data_set(data_dir)?;
    let map = load_map(&data_set, map_index)?;
    let wall_textures = load_wall_textures(&data_set)?;

    if wall_textures.is_empty() {
        return Err(DataError::InvalidFormat(
            "VSWAP contains no wall texture pages".to_string(),
        ));
    }

    Ok(GameAssets {
        map,
        wall_textures,
        source_extension: data_set.extension,
    })
}

fn detect_data_set(data_dir: &Path) -> Result<DataSetFiles, DataError> {
    let candidates = ["WL6", "WL1", "SDM", "SOD"];

    for ext in candidates {
        let Some(maphead) = find_case_insensitive_file(data_dir, &format!("MAPHEAD.{ext}")) else {
            continue;
        };
        let Some(gamemaps) = find_case_insensitive_file(data_dir, &format!("GAMEMAPS.{ext}")) else {
            continue;
        };
        let Some(vswap) = find_case_insensitive_file(data_dir, &format!("VSWAP.{ext}")) else {
            continue;
        };

        return Ok(DataSetFiles {
            extension: ext.to_string(),
            maphead,
            gamemaps,
            vswap,
        });
    }

    Err(DataError::NoSupportedDataSet(data_dir.to_path_buf()))
}

fn find_case_insensitive_file(data_dir: &Path, expected_name: &str) -> Option<PathBuf> {
    let exact = data_dir.join(expected_name);
    if exact.is_file() {
        return Some(exact);
    }

    let entries = fs::read_dir(data_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if name.eq_ignore_ascii_case(expected_name) {
            return Some(path);
        }
    }

    None
}

fn load_map(data_set: &DataSetFiles, map_index: usize) -> Result<TileMap, DataError> {
    let maphead = read_file(&data_set.maphead)?;

    if maphead.len() < 2 + MAP_HEADER_COUNT * 4 {
        return Err(DataError::InvalidFormat(
            "MAPHEAD is too short".to_string(),
        ));
    }

    let rlew_tag = read_u16(&maphead, 0)?;
    let mut header_offsets = Vec::with_capacity(MAP_HEADER_COUNT);

    for i in 0..MAP_HEADER_COUNT {
        let at = 2 + i * 4;
        header_offsets.push(read_u32(&maphead, at)?);
    }

    let map_offset = *header_offsets.get(map_index).ok_or_else(|| {
        DataError::InvalidFormat(format!("map index {map_index} is out of MAPHEAD range"))
    })?;

    if map_offset == 0 || map_offset == u32::MAX {
        return Err(DataError::InvalidFormat(format!(
            "map index {map_index} points to sparse/empty entry"
        )));
    }

    let gamemaps = read_file(&data_set.gamemaps)?;

    let header = parse_map_header(&gamemaps, map_offset as usize)?;
    let tile_count = header.width as usize * header.height as usize;

    if tile_count == 0 {
        return Err(DataError::InvalidFormat(
            "map header has zero width/height".to_string(),
        ));
    }

    let raw_walls = decode_plane(
        &gamemaps,
        header.plane_start[0] as usize,
        header.plane_len[0] as usize,
        rlew_tag,
        tile_count,
    )?;

    let info = decode_plane(
        &gamemaps,
        header.plane_start[1] as usize,
        header.plane_len[1] as usize,
        rlew_tag,
        tile_count,
    )?;

    let walls = normalize_wall_plane(&raw_walls);
    let doors = detect_doors(&walls, header.width as usize, header.height as usize);
    let enemies = detect_enemies(&info, header.width as usize, header.height as usize);
    let player_start = detect_player_start(&info, header.width as usize, header.height as usize);

    Ok(TileMap {
        width: header.width as usize,
        height: header.height as usize,
        walls,
        info,
        player_start,
        doors,
        enemies,
    })
}

fn normalize_wall_plane(raw: &[u16]) -> Vec<u16> {
    raw.iter()
        .map(|tile| {
            if (90..=101).contains(tile) {
                *tile
            } else if *tile == AMBUSH_TILE || *tile >= AREA_TILE {
                0
            } else {
                *tile
            }
        })
        .collect()
}

fn detect_doors(walls: &[u16], width: usize, height: usize) -> Vec<DoorSpawn> {
    let mut doors = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let tile = walls[y * width + x];
            if !(90..=101).contains(&tile) {
                continue;
            }

            let even = tile % 2 == 0;
            let lock = if even { (tile - 90) / 2 } else { (tile - 91) / 2 } as u8;
            doors.push(DoorSpawn {
                x,
                y,
                vertical: even,
                lock,
                tile,
            });
        }
    }

    doors
}

fn detect_enemies(info_plane: &[u16], width: usize, height: usize) -> Vec<EnemySpawn> {
    let mut enemies = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let tile = info_plane[y * width + x];
            if let Some((kind, patrolling)) = decode_enemy_tile(tile) {
                enemies.push(EnemySpawn {
                    kind,
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                    patrolling,
                });
            }
        }
    }

    enemies
}

fn decode_enemy_tile(tile: u16) -> Option<(EnemyKind, bool)> {
    if matches!(tile, 108..=111 | 144..=147 | 180..=183) {
        return Some((EnemyKind::Guard, false));
    }
    if matches!(tile, 112..=115 | 148..=151 | 184..=187) {
        return Some((EnemyKind::Guard, true));
    }

    if matches!(tile, 116..=119 | 152..=155 | 188..=191) {
        return Some((EnemyKind::Officer, false));
    }
    if matches!(tile, 120..=123 | 156..=159 | 192..=195) {
        return Some((EnemyKind::Officer, true));
    }

    if matches!(tile, 126..=129 | 162..=165 | 198..=201) {
        return Some((EnemyKind::Ss, false));
    }
    if matches!(tile, 130..=133 | 166..=169 | 202..=205) {
        return Some((EnemyKind::Ss, true));
    }

    if matches!(tile, 134..=137 | 170..=173 | 206..=209) {
        return Some((EnemyKind::Dog, false));
    }
    if matches!(tile, 138..=141 | 174..=177 | 210..=213) {
        return Some((EnemyKind::Dog, true));
    }

    if matches!(tile, 216..=219 | 234..=237 | 252..=255) {
        return Some((EnemyKind::Mutant, false));
    }
    if matches!(tile, 220..=223 | 238..=241 | 256..=259) {
        return Some((EnemyKind::Mutant, true));
    }

    None
}

fn parse_map_header(gamemaps: &[u8], offset: usize) -> Result<MapHeader, DataError> {
    let mut cursor = offset;
    if cursor + 38 > gamemaps.len() {
        return Err(DataError::InvalidFormat(
            "GAMEMAPS map header out of bounds".to_string(),
        ));
    }

    let mut plane_start = [0u32; MAP_PLANES];
    for value in &mut plane_start {
        *value = read_u32(gamemaps, cursor)?;
        cursor += 4;
    }

    let mut plane_len = [0u16; MAP_PLANES];
    for value in &mut plane_len {
        *value = read_u16(gamemaps, cursor)?;
        cursor += 2;
    }

    let width = read_u16(gamemaps, cursor)?;
    cursor += 2;
    let height = read_u16(gamemaps, cursor)?;

    Ok(MapHeader {
        plane_start,
        plane_len,
        width,
        height,
    })
}

fn decode_plane(
    gamemaps: &[u8],
    plane_start: usize,
    plane_len: usize,
    rlew_tag: u16,
    tile_count: usize,
) -> Result<Vec<u16>, DataError> {
    if plane_len < 2 {
        return Err(DataError::InvalidFormat(
            "compressed plane has invalid length".to_string(),
        ));
    }

    let end = plane_start
        .checked_add(plane_len)
        .ok_or_else(|| DataError::InvalidFormat("compressed plane overflows bounds".to_string()))?;

    if end > gamemaps.len() {
        return Err(DataError::InvalidFormat(
            "compressed plane is out of GAMEMAPS bounds".to_string(),
        ));
    }

    let compressed = &gamemaps[plane_start..end];
    let expanded_words_len = read_u16(compressed, 0)? as usize;
    let carmack = carmack_expand(&compressed[2..], expanded_words_len)?;

    if carmack.is_empty() {
        return Err(DataError::InvalidFormat(
            "carmack expansion produced no words".to_string(),
        ));
    }

    rlew_expand(&carmack[1..], rlew_tag, tile_count)
}

fn detect_player_start(info_plane: &[u16], width: usize, height: usize) -> PlayerStart {
    for y in 0..height {
        for x in 0..width {
            let tile = info_plane[y * width + x];
            if (PLAYER_SPAWN_MIN..=PLAYER_SPAWN_MAX).contains(&tile) {
                let heading = match tile {
                    19 => -std::f32::consts::FRAC_PI_2,
                    20 => 0.0,
                    21 => std::f32::consts::FRAC_PI_2,
                    22 => std::f32::consts::PI,
                    _ => 0.0,
                };

                return PlayerStart {
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                    heading,
                };
            }
        }
    }

    PlayerStart {
        x: 2.5,
        y: 2.5,
        heading: 0.0,
    }
}

fn load_wall_textures(data_set: &DataSetFiles) -> Result<Vec<WallTexture>, DataError> {
    let vswap = read_file(&data_set.vswap)?;

    if vswap.len() < 6 {
        return Err(DataError::InvalidFormat(
            "VSWAP header is too short".to_string(),
        ));
    }

    let chunk_count = read_u16(&vswap, 0)? as usize;
    let sprite_start = read_u16(&vswap, 2)? as usize;

    let offset_table_start = 6;
    let offset_table_size = chunk_count
        .checked_mul(4)
        .ok_or_else(|| DataError::InvalidFormat("VSWAP chunk offset table overflow".to_string()))?;
    let length_table_start = offset_table_start + offset_table_size;
    let length_table_size = chunk_count
        .checked_mul(2)
        .ok_or_else(|| DataError::InvalidFormat("VSWAP chunk length table overflow".to_string()))?;

    if length_table_start + length_table_size > vswap.len() {
        return Err(DataError::InvalidFormat(
            "VSWAP tables exceed file size".to_string(),
        ));
    }

    let mut offsets = Vec::with_capacity(chunk_count);
    for i in 0..chunk_count {
        offsets.push(read_u32(&vswap, offset_table_start + i * 4)? as usize);
    }

    let mut lengths = Vec::with_capacity(chunk_count);
    for i in 0..chunk_count {
        lengths.push(read_u16(&vswap, length_table_start + i * 2)? as usize);
    }

    let wall_chunk_count = sprite_start.min(chunk_count);
    let mut walls = Vec::with_capacity(wall_chunk_count);

    for i in 0..wall_chunk_count {
        let offset = offsets[i];
        let length = lengths[i];

        if offset == 0 || length == 0 {
            continue;
        }

        let end = offset
            .checked_add(length)
            .ok_or_else(|| DataError::InvalidFormat("VSWAP chunk bounds overflow".to_string()))?;

        if end > vswap.len() {
            return Err(DataError::InvalidFormat(format!(
                "VSWAP wall chunk {i} is out of bounds"
            )));
        }

        let chunk = &vswap[offset..end];
        if chunk.len() < 64 * 64 {
            continue;
        }

        walls.push(WallTexture {
            texels: chunk[..64 * 64].to_vec(),
        });
    }

    Ok(walls)
}

fn carmack_expand(input: &[u8], expanded_words: usize) -> Result<Vec<u16>, DataError> {
    let mut out = Vec::with_capacity(expanded_words);
    let mut i = 0usize;

    while out.len() < expanded_words {
        let ch = read_u16_from_slice(input, &mut i)?;
        let ch_high = (ch >> 8) as u8;

        if ch_high == NEAR_TAG {
            let count = (ch & 0x00FF) as usize;
            if count == 0 {
                let low = *input.get(i).ok_or_else(|| {
                    DataError::InvalidFormat("carmack near-tag escaped byte missing".to_string())
                })?;
                i += 1;
                out.push((ch & 0xFF00) | low as u16);
            } else {
                let back_offset = *input.get(i).ok_or_else(|| {
                    DataError::InvalidFormat("carmack near-tag back offset missing".to_string())
                })? as usize;
                i += 1;

                if back_offset == 0 || back_offset > out.len() {
                    return Err(DataError::InvalidFormat(
                        "carmack near-tag invalid back-reference".to_string(),
                    ));
                }

                let start = out.len() - back_offset;
                for n in 0..count {
                    let value = *out.get(start + n).ok_or_else(|| {
                        DataError::InvalidFormat("carmack near-tag copy out of range".to_string())
                    })?;
                    out.push(value);
                }
            }
        } else if ch_high == FAR_TAG {
            let count = (ch & 0x00FF) as usize;
            if count == 0 {
                let low = *input.get(i).ok_or_else(|| {
                    DataError::InvalidFormat("carmack far-tag escaped byte missing".to_string())
                })?;
                i += 1;
                out.push((ch & 0xFF00) | low as u16);
            } else {
                let offset = read_u16_from_slice(input, &mut i)? as usize;
                if offset >= out.len() {
                    return Err(DataError::InvalidFormat(
                        "carmack far-tag offset out of range".to_string(),
                    ));
                }

                for n in 0..count {
                    let value = *out.get(offset + n).ok_or_else(|| {
                        DataError::InvalidFormat("carmack far-tag copy out of range".to_string())
                    })?;
                    out.push(value);
                }
            }
        } else {
            out.push(ch);
        }
    }

    out.truncate(expanded_words);
    Ok(out)
}

fn rlew_expand(source: &[u16], rlew_tag: u16, expanded_words: usize) -> Result<Vec<u16>, DataError> {
    let mut out = Vec::with_capacity(expanded_words);
    let mut i = 0usize;

    while out.len() < expanded_words {
        let value = *source.get(i).ok_or_else(|| {
            DataError::InvalidFormat("rlew input ended before target length".to_string())
        })?;
        i += 1;

        if value != rlew_tag {
            out.push(value);
            continue;
        }

        let count = *source.get(i).ok_or_else(|| {
            DataError::InvalidFormat("rlew marker missing repeat count".to_string())
        })? as usize;
        i += 1;

        let repeated = *source.get(i).ok_or_else(|| {
            DataError::InvalidFormat("rlew marker missing repeated value".to_string())
        })?;
        i += 1;

        for _ in 0..count {
            out.push(repeated);
        }
    }

    out.truncate(expanded_words);
    Ok(out)
}

fn read_file(path: &Path) -> Result<Vec<u8>, DataError> {
    fs::read(path).map_err(|error| DataError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn read_u16(data: &[u8], index: usize) -> Result<u16, DataError> {
    let bytes = data
        .get(index..index + 2)
        .ok_or_else(|| DataError::InvalidFormat("u16 read out of bounds".to_string()))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], index: usize) -> Result<u32, DataError> {
    let bytes = data
        .get(index..index + 4)
        .ok_or_else(|| DataError::InvalidFormat("u32 read out of bounds".to_string()))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u16_from_slice(data: &[u8], cursor: &mut usize) -> Result<u16, DataError> {
    let value = read_u16(data, *cursor)?;
    *cursor += 2;
    Ok(value)
}
