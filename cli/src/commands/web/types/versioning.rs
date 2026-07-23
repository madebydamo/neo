use serde::Serialize;

#[derive(Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
}

#[derive(Serialize)]
pub struct BranchesContext {
    /// Legacy ASCII graph (unused by the D3 UI; kept empty for template compat).
    pub graph: String,
    pub branches: Vec<BranchInfo>,
}

/// One commit node for the versioning D3 graph (JSON API).
#[derive(Serialize, Clone, Debug)]
pub struct GraphCommit {
    pub id: String,
    #[serde(rename = "shortId")]
    pub short_id: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub timestamp: i64,
    pub branches: Vec<String>,
    #[serde(rename = "isHead")]
    pub is_head: bool,
    /// Linked NixOS generation when recorded at activate time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct VersioningGraph {
    pub commits: Vec<GraphCommit>,
    pub head: String,
    #[serde(rename = "currentBranch")]
    pub current_branch: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ServicesAtRev {
    pub rev: String,
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
}
