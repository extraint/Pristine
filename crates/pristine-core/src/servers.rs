//! The game's own server list (`servers.dat`), world summaries, the server list ping
//! (online count + latency) and the signed-in player's skin. These feed the home screen.

use crate::error::{Error, Result};
use crate::instance::Instance;
use crate::paths::Paths;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

// ---------- minimal NBT (big-endian, uncompressed) ----------

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(u8, Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1, Tag::Short(_) => 2, Tag::Int(_) => 3, Tag::Long(_) => 4, Tag::Float(_) => 5,
            Tag::Double(_) => 6, Tag::ByteArray(_) => 7, Tag::String(_) => 8, Tag::List(..) => 9,
            Tag::Compound(_) => 10, Tag::IntArray(_) => 11, Tag::LongArray(_) => 12,
        }
    }
    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self { Tag::Compound(v) => v.iter().find(|(k, _)| k == key).map(|(_, t)| t), _ => None }
    }
    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? { Tag::String(s) => Some(s), _ => None }
    }
    fn long(&self, key: &str) -> Option<i64> {
        match self.get(key)? { Tag::Long(n) => Some(*n), Tag::Int(n) => Some(*n as i64), _ => None }
    }
}

struct Reader<'a> { b: &'a [u8], i: usize }

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.i.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(|| Error::msg("truncated NBT"))?;
        let s = &self.b[self.i..end];
        self.i = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> { Ok(self.take(1)?[0]) }
    fn i16(&mut self) -> Result<i16> { Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap())) }
    fn i32(&mut self) -> Result<i32> { Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap())) }
    fn i64(&mut self) -> Result<i64> { Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap())) }
    fn len(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > self.b.len() { return Err(Error::msg("bad NBT length")); }
        Ok(n as usize)
    }
    fn string(&mut self) -> Result<String> {
        let n = self.i16()? as u16 as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn payload(&mut self, id: u8, depth: u32) -> Result<Tag> {
        if depth > 64 { return Err(Error::msg("NBT nested too deeply")); }
        Ok(match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => { let n = self.len()?; Tag::ByteArray(self.take(n)?.iter().map(|&b| b as i8).collect()) }
            8 => Tag::String(self.string()?),
            9 => {
                let inner = self.u8()?;
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n { v.push(self.payload(inner, depth + 1)?); }
                Tag::List(inner, v)
            }
            10 => {
                let mut v = Vec::new();
                loop {
                    let t = self.u8()?;
                    if t == 0 { break; }
                    let name = self.string()?;
                    v.push((name, self.payload(t, depth + 1)?));
                }
                Tag::Compound(v)
            }
            11 => { let n = self.len()?; Tag::IntArray((0..n).map(|_| self.i32()).collect::<Result<_>>()?) }
            12 => { let n = self.len()?; Tag::LongArray((0..n).map(|_| self.i64()).collect::<Result<_>>()?) }
            _ => return Err(Error::msg(format!("unknown NBT tag {id}"))),
        })
    }
}

/// Parse a named root tag.
pub fn read_nbt(bytes: &[u8]) -> Result<Tag> {
    let mut r = Reader { b: bytes, i: 0 };
    let id = r.u8()?;
    let _name = r.string()?;
    r.payload(id, 0)
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(u16::MAX as usize);
    out.extend((n as u16).to_be_bytes());
    out.extend(&b[..n]);
}

fn put(out: &mut Vec<u8>, t: &Tag) {
    match t {
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend(v.to_be_bytes()),
        Tag::Int(v) => out.extend(v.to_be_bytes()),
        Tag::Long(v) => out.extend(v.to_be_bytes()),
        Tag::Float(v) => out.extend(v.to_bits().to_be_bytes()),
        Tag::Double(v) => out.extend(v.to_bits().to_be_bytes()),
        Tag::ByteArray(v) => { out.extend((v.len() as i32).to_be_bytes()); out.extend(v.iter().map(|&b| b as u8)); }
        Tag::String(s) => put_str(out, s),
        Tag::List(inner, v) => {
            out.push(if v.is_empty() { *inner } else { v[0].id() });
            out.extend((v.len() as i32).to_be_bytes());
            v.iter().for_each(|t| put(out, t));
        }
        Tag::Compound(v) => {
            for (k, t) in v { out.push(t.id()); put_str(out, k); put(out, t); }
            out.push(0);
        }
        Tag::IntArray(v) => { out.extend((v.len() as i32).to_be_bytes()); v.iter().for_each(|x| out.extend(x.to_be_bytes())); }
        Tag::LongArray(v) => { out.extend((v.len() as i32).to_be_bytes()); v.iter().for_each(|x| out.extend(x.to_be_bytes())); }
    }
}

pub fn write_nbt(root: &Tag) -> Vec<u8> {
    let mut out = vec![root.id()];
    put_str(&mut out, "");
    put(&mut out, root);
    out
}

// ---------- servers.dat ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SavedServer {
    pub name: String,
    pub ip: String,
    /// PNG favicon, base64, as the game last saw it.
    pub icon: Option<String>,
}

fn servers_file(paths: &Paths, inst: &Instance) -> std::path::PathBuf {
    paths.game_dir(&inst.id).join("servers.dat")
}

fn load_root(path: &Path) -> Result<Tag> {
    match std::fs::read(path) {
        Ok(b) => read_nbt(&b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Tag::Compound(vec![("servers".into(), Tag::List(10, vec![]))])),
        Err(e) => Err(e.into()),
    }
}

/// The servers the game shows on its Multiplayer screen, in order, hidden ones left out.
pub fn list_servers(paths: &Paths, inst: &Instance) -> Vec<SavedServer> {
    let Ok(root) = load_root(&servers_file(paths, inst)) else { return vec![] };
    let Some(Tag::List(_, list)) = root.get("servers") else { return vec![] };
    list.iter()
        .filter(|s| !matches!(s.get("hidden"), Some(Tag::Byte(1))))
        .filter_map(|s| Some(SavedServer {
            ip: s.str("ip")?.to_string(),
            name: s.str("name").filter(|n| !n.is_empty()).unwrap_or("Minecraft Server").to_string(),
            icon: s.str("icon").map(String::from).filter(|i| !i.is_empty()),
        }))
        .collect()
}

/// Add a server to the end of the game's list, keeping everything else in the file as it was.
pub fn add_server(paths: &Paths, inst: &Instance, name: &str, ip: &str) -> Result<()> {
    let (name, ip) = (name.trim(), ip.trim());
    if ip.is_empty() || ip.len() > 255 || ip.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(Error::msg("Enter a server address like play.example.net"));
    }
    let path = servers_file(paths, inst);
    let mut root = load_root(&path)?;
    let Tag::Compound(fields) = &mut root else { return Err(Error::msg("servers.dat is not a compound")) };
    if !fields.iter().any(|(k, _)| k == "servers") { fields.push(("servers".into(), Tag::List(10, vec![]))); }
    let Some((_, Tag::List(_, list))) = fields.iter_mut().find(|(k, _)| k == "servers") else { return Err(Error::msg("servers.dat has an odd layout")) };
    if list.iter().any(|s| s.str("ip").is_some_and(|x| x.eq_ignore_ascii_case(ip))) {
        return Err(Error::msg("That server is already in your list"));
    }
    list.push(Tag::Compound(vec![
        ("name".into(), Tag::String(if name.is_empty() { "Minecraft Server".into() } else { name.into() })),
        ("ip".into(), Tag::String(ip.into())),
    ]));
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
    let tmp = path.with_extension("dat_new");
    std::fs::write(&tmp, write_nbt(&root))?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub fn remove_server(paths: &Paths, inst: &Instance, ip: &str) -> Result<()> {
    let path = servers_file(paths, inst);
    let mut root = load_root(&path)?;
    if let Tag::Compound(fields) = &mut root {
        if let Some((_, Tag::List(_, list))) = fields.iter_mut().find(|(k, _)| k == "servers") {
            list.retain(|s| s.str("ip") != Some(ip));
        }
    }
    let tmp = path.with_extension("dat_new");
    std::fs::write(&tmp, write_nbt(&root))?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

// ---------- worlds ----------

#[derive(Debug, Clone, Serialize)]
pub struct WorldInfo {
    /// Folder name, which is what quick play takes.
    pub folder: String,
    pub name: String,
    /// Unix milliseconds.
    pub last_played: Option<i64>,
    /// `icon.png`, base64.
    pub icon: Option<String>,
}

pub fn list_worlds(paths: &Paths, inst: &Instance) -> Vec<WorldInfo> {
    let saves = paths.game_dir(&inst.id).join("saves");
    let Ok(rd) = std::fs::read_dir(&saves) else { return vec![] };
    let mut out: Vec<WorldInfo> = rd.flatten().filter_map(|e| {
        let dir = e.path();
        let level = dir.join("level.dat");
        if !level.is_file() { return None; }
        let folder = e.file_name().to_string_lossy().into_owned();
        let data = std::fs::read(&level).ok().and_then(|raw| {
            let mut buf = Vec::new();
            flate2::read::GzDecoder::new(&raw[..]).take(64 << 20).read_to_end(&mut buf).ok()?;
            read_nbt(&buf).ok()
        });
        let d = data.as_ref().and_then(|t| t.get("Data"));
        let mtime = std::fs::metadata(&level).ok().and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64);
        let icon = std::fs::read(dir.join("icon.png")).ok().filter(|b| b.len() < 512 * 1024).map(|b| STANDARD.encode(b));
        Some(WorldInfo {
            name: d.and_then(|d| d.str("LevelName")).filter(|n| !n.is_empty()).unwrap_or(&folder).to_string(),
            last_played: d.and_then(|d| d.long("LastPlayed")).or(mtime),
            folder,
            icon,
        })
    }).collect();
    out.sort_by_key(|w| std::cmp::Reverse(w.last_played));
    out
}

// ---------- server list ping ----------

#[derive(Debug, Clone, Serialize, Default)]
pub struct ServerStatus {
    pub online: Option<i64>,
    pub max: Option<i64>,
    pub version: Option<String>,
    /// Round trip in milliseconds.
    pub ping_ms: Option<u64>,
    /// `data:image/png;base64,...` as the server sent it.
    pub favicon: Option<String>,
    pub motd: Option<String>,
}

fn varint(mut v: i32, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v = ((v as u32) >> 7) as i32;
        if v != 0 { b |= 0x80; }
        out.push(b);
        if v == 0 { break; }
    }
}

fn packet(id: i32, body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::new();
    varint(id, &mut inner);
    inner.extend(body);
    let mut out = Vec::new();
    varint(inner.len() as i32, &mut out);
    out.extend(inner);
    out
}

async fn read_varint<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> Result<i32> {
    use tokio::io::AsyncReadExt;
    let mut v: u32 = 0;
    for i in 0..5 {
        let b = r.read_u8().await?;
        v |= ((b & 0x7f) as u32) << (7 * i);
        if b & 0x80 == 0 { return Ok(v as i32); }
    }
    Err(Error::msg("bad varint from server"))
}

fn flatten_motd(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(o) => {
            let mut s = o.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();
            if let Some(serde_json::Value::Array(extra)) = o.get("extra") { extra.iter().for_each(|e| s.push_str(&flatten_motd(e))); }
            s
        }
        serde_json::Value::Array(a) => a.iter().map(flatten_motd).collect(),
        _ => String::new(),
    }
}

/// Many hosted servers publish only a `_minecraft._tcp` SRV record, which the game
/// follows when the address has no port. Do the same, so their status shows up.
async fn srv_target(address: &str, host: &str) -> Option<(String, u16)> {
    let has_port = address.trim().rsplit_once(':').is_some_and(|(h, p)| !h.ends_with(':') && p.parse::<u16>().is_ok());
    if has_port || host.parse::<std::net::IpAddr>().is_ok() || host.eq_ignore_ascii_case("localhost") { return None; }
    let resolver = hickory_resolver::TokioAsyncResolver::tokio_from_system_conf().ok()?;
    let lookup = resolver.srv_lookup(format!("_minecraft._tcp.{host}."));
    let found = tokio::time::timeout(Duration::from_secs(2), lookup).await.ok()?.ok()?;
    let rec = found.iter().min_by_key(|r| (r.priority(), std::cmp::Reverse(r.weight())))?;
    let target = rec.target().to_utf8();
    let target = target.trim_end_matches('.');
    (!target.is_empty()).then(|| (target.to_string(), rec.port()))
}

/// Ask a server for its status the way the game's Multiplayer screen does.
pub async fn ping(address: &str) -> Result<ServerStatus> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (host, port) = crate::launch::split_address(address);
    let t = Duration::from_secs(5);
    let fut = async {
        let (to_host, to_port) = srv_target(address, &host).await.unwrap_or_else(|| (host.clone(), port));
        let mut s = tokio::net::TcpStream::connect((to_host.as_str(), to_port)).await?;
        let mut hs = Vec::new();
        varint(-1, &mut hs); // protocol version: "any" for a status query
        varint(host.len() as i32, &mut hs);
        hs.extend(host.as_bytes());
        hs.extend(port.to_be_bytes());
        varint(1, &mut hs); // next state: status
        s.write_all(&packet(0, &hs)).await?;
        s.write_all(&packet(0, &[])).await?;
        let _len = read_varint(&mut s).await?;
        if read_varint(&mut s).await? != 0 { return Err(Error::msg("unexpected status reply")); }
        let n = read_varint(&mut s).await?;
        if !(0..=1 << 21).contains(&n) { return Err(Error::msg("status reply too large")); }
        let mut json = vec![0u8; n as usize];
        s.read_exact(&mut json).await?;
        let v: serde_json::Value = serde_json::from_slice(&json).map_err(|e| Error::msg(e.to_string()))?;
        let started = Instant::now();
        s.write_all(&packet(1, &42i64.to_be_bytes())).await?;
        let ping_ms = async {
            let _ = read_varint(&mut s).await?;
            let _ = read_varint(&mut s).await?;
            s.read_i64().await?;
            Ok::<_, Error>(started.elapsed().as_millis() as u64)
        }.await.ok();
        Ok::<_, Error>(ServerStatus {
            online: v.pointer("/players/online").and_then(|x| x.as_i64()),
            max: v.pointer("/players/max").and_then(|x| x.as_i64()),
            version: v.pointer("/version/name").and_then(|x| x.as_str()).map(String::from),
            favicon: v.get("favicon").and_then(|x| x.as_str()).filter(|f| f.starts_with("data:image/png;base64,")).map(String::from),
            motd: v.get("description").map(flatten_motd).map(|m| strip_codes(&m)),
            ping_ms,
        })
    };
    tokio::time::timeout(t, fut).await.map_err(|_| Error::msg("Server didn't answer"))?
}

/// Drop `§x` formatting codes.
fn strip_codes(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() { if c == '§' { it.next(); } else { out.push(c); } }
    out
}

// ---------- the signed-in player's skin ----------

#[derive(Debug, Clone, Serialize)]
pub struct PlayerSkin {
    /// PNG, base64.
    pub data: String,
    pub slim: bool,
}

/// Fetch a player's current skin from Mojang's session server.
pub async fn player_skin(client: &reqwest::Client, uuid: &str) -> Result<PlayerSkin> {
    let uuid: String = uuid.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if uuid.len() != 32 { return Err(Error::msg("bad uuid")); }
    let profile: serde_json::Value = client
        .get(format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"))
        .send().await?.error_for_status()?.json().await?;
    let encoded = profile.get("properties").and_then(|p| p.as_array())
        .and_then(|a| a.iter().find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures")))
        .and_then(|p| p.get("value")).and_then(|v| v.as_str())
        .ok_or_else(|| Error::msg("no skin on this profile"))?;
    let tex: serde_json::Value = serde_json::from_slice(&STANDARD.decode(encoded).map_err(|e| Error::msg(e.to_string()))?)
        .map_err(|e| Error::msg(e.to_string()))?;
    let skin = tex.pointer("/textures/SKIN").ok_or_else(|| Error::msg("no skin on this profile"))?;
    let url = skin.get("url").and_then(|u| u.as_str()).ok_or_else(|| Error::msg("no skin url"))?;
    // Only ever fetch from Mojang's texture host.
    let url = url.replacen("http://", "https://", 1);
    if !url.starts_with("https://textures.minecraft.net/") { return Err(Error::msg("unexpected skin host")); }
    let bytes = client.get(&url).send().await?.error_for_status()?.bytes().await?;
    Ok(PlayerSkin {
        data: STANDARD.encode(&bytes),
        slim: skin.pointer("/metadata/model").and_then(|m| m.as_str()) == Some("slim"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::Loader;

    fn inst(paths: &Paths) -> Instance {
        Instance::create_with(paths, "Test", "1.21.4", Loader::Fabric).unwrap()
    }

    fn tmp() -> Paths {
        let d = std::env::temp_dir().join(format!("pristine-servers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        Paths::new(d)
    }

    #[test]
    fn nbt_round_trips() {
        let t = Tag::Compound(vec![
            ("servers".into(), Tag::List(10, vec![Tag::Compound(vec![
                ("name".into(), Tag::String("Hub".into())), ("ip".into(), Tag::String("mc.example.net".into())),
                ("acceptTextures".into(), Tag::Byte(1)), ("n".into(), Tag::LongArray(vec![1, -2])),
            ])])),
            ("d".into(), Tag::Double(1.5)),
        ]);
        assert_eq!(read_nbt(&write_nbt(&t)).unwrap(), t);
    }

    #[test]
    fn add_list_and_remove_servers() {
        let paths = tmp();
        let i = inst(&paths);
        assert!(list_servers(&paths, &i).is_empty());
        add_server(&paths, &i, "Hub", "mc.example.net").unwrap();
        add_server(&paths, &i, "", "10.0.0.2:25570").unwrap();
        assert!(add_server(&paths, &i, "dup", "MC.EXAMPLE.NET").is_err());
        let l = list_servers(&paths, &i);
        assert_eq!(l.iter().map(|s| (s.name.as_str(), s.ip.as_str())).collect::<Vec<_>>(),
            vec![("Hub", "mc.example.net"), ("Minecraft Server", "10.0.0.2:25570")]);
        remove_server(&paths, &i, "mc.example.net").unwrap();
        assert_eq!(list_servers(&paths, &i).len(), 1);
    }

    #[tokio::test]
    async fn srv_lookup_skips_ports_and_ips() {
        assert!(srv_target("127.0.0.1", "127.0.0.1").await.is_none());
        assert!(srv_target("play.example.net:25570", "play.example.net").await.is_none());
        assert!(srv_target("[::1]:25565", "::1").await.is_none());
    }

    #[test]
    fn reads_worlds() {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let paths = tmp();
        let i = inst(&paths);
        let w = paths.game_dir(&i.id).join("saves").join("my-world");
        std::fs::create_dir_all(&w).unwrap();
        let level = Tag::Compound(vec![("Data".into(), Tag::Compound(vec![
            ("LevelName".into(), Tag::String("Snow Village".into())), ("LastPlayed".into(), Tag::Long(1_700_000_000_000)),
        ]))]);
        let mut gz = GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&write_nbt(&level)).unwrap();
        std::fs::write(w.join("level.dat"), gz.finish().unwrap()).unwrap();
        let worlds = list_worlds(&paths, &i);
        assert_eq!(worlds.len(), 1);
        assert_eq!((worlds[0].folder.as_str(), worlds[0].name.as_str(), worlds[0].last_played), ("my-world", "Snow Village", Some(1_700_000_000_000)));
    }

    #[tokio::test]
    async fn pings_a_fake_server() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            // handshake + status request
            for _ in 0..2 { let n = read_varint(&mut s).await.unwrap(); let mut b = vec![0; n as usize]; s.read_exact(&mut b).await.unwrap(); }
            let json = r#"{"version":{"name":"1.21.4"},"players":{"online":500,"max":1000},"description":{"text":"§aHello","extra":[{"text":" there"}]}}"#.as_bytes();
            let mut body = Vec::new(); varint(json.len() as i32, &mut body); body.extend(json);
            s.write_all(&packet(0, &body)).await.unwrap();
            let n = read_varint(&mut s).await.unwrap(); let mut b = vec![0; n as usize]; s.read_exact(&mut b).await.unwrap();
            s.write_all(&packet(1, &b[1..])).await.unwrap();
        });
        let st = ping(&addr.to_string()).await.unwrap();
        assert_eq!((st.online, st.max, st.version.as_deref(), st.motd.as_deref()), (Some(500), Some(1000), Some("1.21.4"), Some("Hello there")));
        assert!(st.ping_ms.is_some());
    }
}
