//! Minimal Modrinth API v2 client (search, projects, versions).

use crate::download::fetch_json;
use crate::error::{Error, Result};
use crate::loader::Loader;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
    DataPack,
}

impl Kind {
    /// Folder (relative to the game dir, or to a world for data packs) content goes in.
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Mod => "mods",
            Kind::ResourcePack => "resourcepacks",
            Kind::Shader => "shaderpacks",
            Kind::DataPack => "datapacks",
            Kind::Modpack => "",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchParams {
    pub query: String,
    pub kind: Kind,
    pub mc_version: Option<String>,
    pub loader: Option<Loader>,
    #[serde(default)]
    pub offset: u32,
    /// relevance | downloads | follows | newest | updated
    pub index: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub hits: Vec<Hit>,
    pub total: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub dependency_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hashes {
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub hashes: Hashes,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub files: Vec<VFile>,
}

impl Version {
    pub fn primary_file(&self) -> Option<&VFile> {
        self.files.iter().find(|f| f.primary).or(self.files.first())
    }
}

/// The API returns newest first; prefer the newest release, then beta, then anything.
pub fn best_version(versions: &[Version]) -> Option<&Version> {
    ["release", "beta"].iter()
        .find_map(|t| versions.iter().find(|v| v.version_type == *t))
        .or(versions.first())
}

/// Loader filter for a kind of content (only mods care which loader they target).
pub fn loaders_for(kind: Kind, loader: Loader) -> Result<Option<Vec<String>>> {
    Ok(match kind {
        Kind::Mod => Some(match loader {
            Loader::Fabric => vec!["fabric".into()],
            Loader::Quilt => vec!["quilt".into(), "fabric".into()],
            Loader::Vanilla => {
                return Err(Error::msg("Mods need Fabric or Quilt. Change this instance's loader in its settings."))
            }
        }),
        Kind::DataPack => Some(vec!["datapack".into()]),
        _ => None,
    })
}

fn facets(p: &SearchParams) -> String {
    let mut f: Vec<Vec<String>> = vec![];
    f.push(vec![format!("project_type:{}", if p.kind == Kind::DataPack { "mod" } else { kind_str(p.kind) })]);
    match p.kind {
        Kind::DataPack => f.push(vec!["categories:datapack".into()]),
        Kind::Mod | Kind::Modpack => match p.loader {
            Some(Loader::Fabric) => f.push(vec!["categories:fabric".into()]),
            Some(Loader::Quilt) => f.push(vec!["categories:quilt".into(), "categories:fabric".into()]),
            _ => {}
        },
        _ => {}
    }
    if let Some(v) = &p.mc_version { f.push(vec![format!("versions:{v}")]); }
    json!(f).to_string()
}

fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Mod | Kind::DataPack => "mod",
        Kind::Modpack => "modpack",
        Kind::ResourcePack => "resourcepack",
        Kind::Shader => "shader",
    }
}

pub async fn search(client: &reqwest::Client, p: &SearchParams) -> Result<SearchResult> {
    #[derive(Deserialize)]
    struct Resp { hits: Vec<Hit>, total_hits: u64 }
    let r: Resp = client.get(format!("{API}/search"))
        .query(&[
            ("query", p.query.as_str()),
            ("facets", facets(p).as_str()),
            ("limit", "24"),
            ("offset", p.offset.to_string().as_str()),
            ("index", p.index.as_deref().unwrap_or(if p.query.is_empty() { "downloads" } else { "relevance" })),
        ])
        .send().await?.error_for_status()?.json().await?;
    Ok(SearchResult { hits: r.hits, total: r.total_hits })
}

pub async fn project(client: &reqwest::Client, id_or_slug: &str) -> Result<Project> {
    fetch_json(client, &format!("{API}/project/{id_or_slug}")).await
        .map_err(|_| Error::msg(format!("Couldn't find '{id_or_slug}' on Modrinth")))
}

pub async fn versions(
    client: &reqwest::Client,
    id_or_slug: &str,
    mc_version: Option<&str>,
    loaders: Option<&[String]>,
) -> Result<Vec<Version>> {
    let mut q: Vec<(&str, String)> = vec![];
    if let Some(v) = mc_version { q.push(("game_versions", json!([v]).to_string())); }
    if let Some(l) = loaders { q.push(("loaders", json!(l).to_string())); }
    Ok(client.get(format!("{API}/project/{id_or_slug}/version")).query(&q)
        .send().await?.error_for_status()?.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(kind: Kind, loader: Option<Loader>) -> SearchParams {
        SearchParams { query: String::new(), kind, mc_version: Some("1.21.1".into()), loader, offset: 0, index: None }
    }

    #[test]
    fn facets_for_each_kind() {
        assert_eq!(facets(&p(Kind::Mod, Some(Loader::Fabric))),
            r#"[["project_type:mod"],["categories:fabric"],["versions:1.21.1"]]"#);
        assert_eq!(facets(&p(Kind::DataPack, None)),
            r#"[["project_type:mod"],["categories:datapack"],["versions:1.21.1"]]"#);
        assert_eq!(facets(&p(Kind::Shader, Some(Loader::Fabric))),
            r#"[["project_type:shader"],["versions:1.21.1"]]"#);
    }

    #[test]
    fn mods_need_a_loader() {
        assert!(loaders_for(Kind::Mod, Loader::Vanilla).is_err());
        assert_eq!(loaders_for(Kind::Mod, Loader::Quilt).unwrap().unwrap(), ["quilt", "fabric"]);
        assert!(loaders_for(Kind::ResourcePack, Loader::Vanilla).unwrap().is_none());
    }

    #[test]
    fn prefers_release() {
        let v = |id: &str, t: &str| Version { id: id.into(), project_id: "p".into(), name: id.into(), version_number: id.into(),
            version_type: t.into(), game_versions: vec![], dependencies: vec![], files: vec![] };
        let list = vec![v("b1", "beta"), v("r1", "release"), v("a1", "alpha")];
        assert_eq!(best_version(&list).unwrap().id, "r1");
        assert_eq!(best_version(&list[..1]).unwrap().id, "b1");
    }
}
